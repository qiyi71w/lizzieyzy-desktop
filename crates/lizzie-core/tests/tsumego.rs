//! 死活题 and 抓取死活题: the frame construction against the fixed Java build's own regression
//! expectations (`TsumegoTest`), the screen recognition against synthetic pictures, and the
//! workbench commands through the public `Workbench::edit` operation.

use lizzie_core::api::{CommandResult, EditCommand};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::formats;
use lizzie_core::record::{BoardSize, GameRecord, Point, RecordFormat, Stone, ROOT};
use lizzie_core::settings::{CaptureSettings, Storage, TsumegoFrameSettings};
use lizzie_core::tsumego::capture::{self, CapturedProblem, Picture, Selection};
use lizzie_core::tsumego::{build_frame, FrameOptions, TsumegoAttacker, TsumegoKoThreat, TsumegoToPlay};
use lizzie_core::workbench::{OpenOrigin, Workbench};

const SIZE: BoardSize = BoardSize::STANDARD;

// ---------------------------------------------------------------------------- frame

fn board(stones: &[(Stone, &[(u16, u16)])]) -> Vec<Option<Stone>> {
    let mut b = vec![None; SIZE.area()];
    for (color, points) in stones {
        for &(x, y) in *points {
            b[SIZE.index(Point { x, y })] = Some(*color);
        }
    }
    b
}

const B: Stone = Stone::Black;
const W: Stone = Stone::White;

fn black_surrounds_white() -> Vec<Option<Stone>> {
    board(&[
        (W, &[(9, 9), (10, 9)]),
        (
            B,
            &[
                (8, 8),
                (9, 8),
                (10, 8),
                (11, 8),
                (8, 9),
                (11, 9),
                (8, 10),
                (9, 10),
                (10, 10),
                (11, 10),
            ],
        ),
    ])
}

fn options(wall: i32, attacker: TsumegoAttacker, ko: TsumegoKoThreat, to_play: TsumegoToPlay) -> FrameOptions {
    FrameOptions {
        to_play,
        attacker,
        ko_threat: ko,
        wall_distance: wall,
    }
}

fn plain(wall: i32) -> FrameOptions {
    options(wall, TsumegoAttacker::Auto, TsumegoKoThreat::None, TsumegoToPlay::Keep)
}

fn framed(stones: &[Option<Stone>], o: FrameOptions) -> Vec<Option<Stone>> {
    let frame = build_frame(SIZE, stones, true, &o).frame.expect("a frame");
    assert_originals_kept(stones, &frame.stones);
    frame.stones
}

fn at(stones: &[Option<Stone>], x: u16, y: u16) -> Option<Stone> {
    stones[SIZE.index(Point { x, y })]
}

fn assert_originals_kept(before: &[Option<Stone>], after: &[Option<Stone>]) {
    for (i, s) in before.iter().enumerate() {
        if s.is_some() {
            assert_eq!(after[i], *s, "problem stone {i} changed");
        }
    }
}

/// Java `stoneDeltas`: `x,y=COLOR`, x outer, y inner.
fn deltas(base: &[Option<Stone>], actual: &[Option<Stone>]) -> Vec<String> {
    let mut out = Vec::new();
    for x in 0..SIZE.width {
        for y in 0..SIZE.height {
            let (a, b) = (at(base, x, y), at(actual, x, y));
            if a != b {
                let c = match b {
                    None => "EMPTY",
                    Some(Stone::Black) => "BLACK",
                    Some(Stone::White) => "WHITE",
                };
                out.push(format!("{x},{y}={c}"));
            }
        }
    }
    out
}

#[test]
fn automatic_attacker_is_the_colour_with_more_stones_and_black_on_a_tie() {
    let attacker = |b: &[Option<Stone>]| build_frame(SIZE, b, true, &plain(1)).attacker;
    assert_eq!(attacker(&black_surrounds_white()), B);
    let white_surrounds = board(&[
        (B, &[(9, 9), (10, 9)]),
        (
            W,
            &[
                (8, 8),
                (9, 8),
                (10, 8),
                (11, 8),
                (8, 9),
                (11, 9),
                (8, 10),
                (9, 10),
                (10, 10),
                (11, 10),
            ],
        ),
    ]);
    assert_eq!(attacker(&white_surrounds), W);
    let tie = board(&[(B, &[(8, 8), (9, 8)]), (W, &[(8, 9), (9, 9)])]);
    assert_eq!(attacker(&tie), B);
    let forced = |a| {
        build_frame(
            SIZE,
            &black_surrounds_white(),
            true,
            &options(1, a, TsumegoKoThreat::None, TsumegoToPlay::Keep),
        )
        .attacker
    };
    assert_eq!(forced(TsumegoAttacker::White), W);
    assert_eq!(forced(TsumegoAttacker::Black), B);
}

#[test]
fn walls_stay_on_the_board_around_corner_edge_and_centre_problems() {
    let corner = board(&[
        (W, &[(1, 1), (2, 1)]),
        (B, &[(0, 0), (0, 1), (0, 2), (1, 2), (2, 2), (3, 1)]),
    ]);
    let edge = board(&[
        (W, &[(0, 8), (1, 8)]),
        (B, &[(0, 7), (1, 7), (2, 7), (2, 8), (2, 9), (0, 9), (1, 9)]),
    ]);
    let count = |s: &[Option<Stone>]| s.iter().filter(|s| s.is_some()).count();
    let c = framed(&corner, plain(1));
    let e = framed(&edge, plain(1));
    let m = framed(&black_surrounds_white(), plain(1));
    assert_eq!(at(&c, 5, 2), Some(B));
    assert_eq!(at(&e, 4, 8), Some(B));
    assert_eq!(at(&m, 6, 9), Some(B));
    assert!(count(&c) > 6 && count(&e) > 9 && count(&m) > 12);
}

#[test]
fn wall_distance_moves_the_wall_and_the_forced_attacker_builds_it() {
    let close = framed(&black_surrounds_white(), plain(1));
    let far = framed(&black_surrounds_white(), plain(3));
    assert_ne!(close, far);
    // The problem spans x 8..11; distance 1 walls x 6, distance 3 leaves it empty and walls x 4.
    assert_eq!(at(&close, 6, 9), Some(B));
    assert_eq!(at(&far, 6, 9), None);
    assert_eq!(at(&far, 4, 9), Some(B));
    let white = framed(
        &black_surrounds_white(),
        options(1, TsumegoAttacker::White, TsumegoKoThreat::None, TsumegoToPlay::Keep),
    );
    assert_eq!(at(&white, 6, 9), Some(W));
}

#[test]
fn ko_threats_are_corner_shapes_for_the_defender_and_the_attacker() {
    let base = black_surrounds_white();
    let with = |ko| framed(&base, options(1, TsumegoAttacker::Auto, ko, TsumegoToPlay::Keep));
    let none = with(TsumegoKoThreat::None);
    let defender = [
        "15,17=EMPTY",
        "16,17=WHITE",
        "17,17=WHITE",
        "17,18=WHITE",
        "18,17=WHITE",
        "18,18=EMPTY",
    ];
    let attacker = [
        "0,0=EMPTY",
        "0,1=BLACK",
        "1,0=BLACK",
        "1,1=BLACK",
        "2,1=BLACK",
        "3,1=EMPTY",
    ];
    assert_eq!(deltas(&none, &with(TsumegoKoThreat::Defender)), defender);
    assert_eq!(deltas(&none, &with(TsumegoKoThreat::Attacker)), attacker);
    let both: Vec<&str> = attacker.iter().chain(defender.iter()).copied().collect();
    assert_eq!(deltas(&none, &with(TsumegoKoThreat::Both)), both);
}

#[test]
fn a_ko_threat_without_room_changes_nothing_and_is_reported() {
    let mut tall = Vec::new();
    for y in 1..=17 {
        tall.push((B, (0, y)));
        tall.push((W, (1, y)));
    }
    let mut b = vec![None; SIZE.area()];
    for (c, (x, y)) in tall {
        b[SIZE.index(Point { x, y })] = Some(c);
    }
    let none = build_frame(SIZE, &b, true, &plain(1)).frame.unwrap();
    let ko = build_frame(
        SIZE,
        &b,
        true,
        &options(1, TsumegoAttacker::Auto, TsumegoKoThreat::Defender, TsumegoToPlay::Keep),
    )
    .frame
    .unwrap();
    assert!(deltas(&none.stones, &ko.stones).is_empty());
    assert!(none.no_room.is_empty());
    assert!(!ko.no_room.is_empty());
}

#[test]
fn side_to_play_is_kept_or_forced() {
    let to_play = |black: bool, choice| {
        build_frame(
            SIZE,
            &black_surrounds_white(),
            black,
            &options(1, TsumegoAttacker::Auto, TsumegoKoThreat::None, choice),
        )
        .frame
        .unwrap()
        .black_to_play
    };
    assert!(!to_play(false, TsumegoToPlay::Keep));
    assert!(to_play(false, TsumegoToPlay::Black));
    assert!(!to_play(true, TsumegoToPlay::White));
}

/// Java's arithmetic: a corner problem fits in half the board plus four points and the attacker
/// gets 7.5; the centre problem's cover box (x 6..13, y 6..12 at distance 1) is smallest towards
/// the right (13 x 19 = 247 > 185), so the attacker owns 13 x 13 = 169 points against 181 and the
/// komi is 2 x (169 - 181) = -24 for Black, negated when White attacks.
#[test]
fn komi_follows_the_attackers_share_of_the_board() {
    let corner = board(&[
        (W, &[(1, 1), (2, 1)]),
        (B, &[(0, 0), (0, 1), (0, 2), (1, 2), (2, 2), (3, 1)]),
    ]);
    assert_eq!(build_frame(SIZE, &corner, true, &plain(1)).frame.unwrap().komi, 7.5);
    let black = build_frame(SIZE, &black_surrounds_white(), true, &plain(1))
        .frame
        .unwrap();
    assert_eq!(black.komi, -24.0);
    let white = build_frame(
        SIZE,
        &black_surrounds_white(),
        true,
        &options(1, TsumegoAttacker::White, TsumegoKoThreat::None, TsumegoToPlay::Keep),
    )
    .frame
    .unwrap();
    assert_eq!(white.komi, 24.0);
}

#[test]
fn positions_reaching_every_edge_are_not_framed_and_others_stay_in_bounds() {
    let empty = build_frame(SIZE, &vec![None; SIZE.area()], true, &plain(1));
    assert!(empty.frame.is_some());
    let single = build_frame(SIZE, &board(&[(B, &[(0, 0)])]), true, &plain(1));
    assert_eq!(single.attacker, B);
    assert!(single.frame.is_some());
    let edges = board(&[(B, &[(0, 9), (18, 9), (9, 0)]), (W, &[(9, 18)])]);
    assert!(build_frame(SIZE, &edges, true, &plain(1)).frame.is_none());
    let mut full = vec![None; SIZE.area()];
    for x in 0..19u16 {
        for y in 0..19u16 {
            if (x, y) != (9, 9) {
                full[SIZE.index(Point { x, y })] = Some(if (x + y) % 2 == 0 { B } else { W });
            }
        }
    }
    let both = options(1, TsumegoAttacker::Auto, TsumegoKoThreat::Both, TsumegoToPlay::Keep);
    assert!(build_frame(SIZE, &full, true, &both).frame.is_none());
}

// ---------------------------------------------------------------------------- capture

const WOOD: [u8; 3] = [220, 179, 92];

/// A picture of wood with stones drawn as discs of radius 8 at grid points.
struct Canvas {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
}

impl Canvas {
    fn new(w: u32, h: u32) -> Canvas {
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for _ in 0..w * h {
            rgba.extend_from_slice(&[WOOD[0], WOOD[1], WOOD[2], 255]);
        }
        Canvas { w, h, rgba }
    }

    fn set(&mut self, x: i32, y: i32, c: [u8; 3]) {
        if x >= 0 && y >= 0 && (x as u32) < self.w && (y as u32) < self.h {
            let i = ((y as u32 * self.w + x as u32) * 4) as usize;
            self.rgba[i..i + 3].copy_from_slice(&c);
        }
    }

    fn disc(&mut self, cx: i32, cy: i32, c: [u8; 3]) {
        for y in cy - 8..=cy + 8 {
            for x in cx - 8..=cx + 8 {
                if (x - cx).pow(2) + (y - cy).pow(2) <= 64 {
                    self.set(x, y, c);
                }
            }
        }
    }

    fn block(&mut self, x0: i32, y0: i32, size: i32, c: [u8; 3]) {
        for y in y0..y0 + size {
            for x in x0..x0 + size {
                self.set(x, y, c);
            }
        }
    }

    fn picture(&self) -> Picture<'_> {
        Picture {
            width: self.w,
            height: self.h,
            rgba: &self.rgba,
        }
    }
}

#[test]
fn step_two_gives_the_spacing_and_the_corner_or_cancels_on_a_zero_spacing() {
    assert_eq!(capture::spacing((20, 20), (40, 41)), Some((20, 21)));
    assert_eq!(capture::spacing((20, 20), (20, 40)), None);
    assert_eq!(capture::spacing((20, 20), (40, 20)), None);
    assert_eq!(capture::direction((20, 20), (40, 40)), 1);
    assert_eq!(capture::direction((20, 20), (0, 40)), 2);
    assert_eq!(capture::direction((20, 20), (40, 0)), 3);
    assert_eq!(capture::direction((20, 20), (0, 0)), 4);
}

#[test]
fn the_marked_area_fits_whole_squares_and_needs_half_a_square() {
    // 83 px at about 20 px a square is four squares of 20.75.
    let s = Selection::new((20, 20), (20, 20), (103, 60)).unwrap();
    assert_eq!((s.board_width(), s.board_height()), (5, 3));
    assert!(Selection::new((20, 20), (20, 20), (29, 60)).is_none());
    assert!(Selection::new((20, 20), (20, 20), (20, 60)).is_none());
}

#[test]
fn stones_are_read_from_dark_and_shaded_light_discs_but_not_flat_white() {
    let mut c = Canvas::new(200, 120);
    // Grid origin (20,20), spacing 20: 5 x 3 intersections.
    c.disc(40, 20, [20, 20, 20]); // (1,0) black
    c.disc(60, 40, [250, 250, 250]); // (2,1) white disc on wood
    c.block(90, 70, 20, [255, 255, 255]); // (4,2) flat white area: not a stone
    let shaded = [205, 205, 205]; // (0,2) light gray, all between 190 and 224: shading
    c.block(10, 50, 20, shaded);
    let s = Selection::new((20, 20), (20, 20), (100, 60)).unwrap();
    let settings = CaptureSettings::default();
    let stones = s.recognize(&c.picture(), &settings);
    #[rustfmt::skip]
    let expected = vec![
        0, 1, 0, 0, 0,
        0, 0, 2, 0, 0,
        2, 0, 0, 0, 0,
    ];
    assert_eq!(stones, expected);
    // A higher black percentage than the disc covers reads it as empty.
    let strict = CaptureSettings {
        black_percent: 90,
        ..settings
    };
    assert_eq!(s.recognize(&c.picture(), &strict)[1], 0);
    let problem = s.problem(capture::direction((20, 20), (40, 40)), &c.picture(), &settings);
    assert_eq!(
        problem,
        CapturedProblem {
            direction: 1,
            width: 5,
            height: 3,
            stones: expected
        }
    );
}

// ---------------------------------------------------------------------------- workbench

struct NoEngines;

impl Launcher for NoEngines {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        Err(std::io::Error::other("no engines in tsumego tests"))
    }
}

fn opened(dir: &std::path::Path, text: &str) -> Workbench {
    let storage = Storage::in_dirs(&dir.join("config"), &dir.join("data"));
    let mut wb = Workbench::new(storage, None, Box::new(NoEngines), Vec::new());
    let read = formats::parse(RecordFormat::Sgf, text, false);
    let t = wb.begin_open(OpenOrigin::File {
        path: "/x.sgf".into(),
        source: lizzie_core::api::OpenSource::Drop,
    });
    assert!(wb.complete_open(t, read.map(|r| (RecordFormat::Sgf, r))));
    wb
}

/// The black-surrounds-white problem played as moves after setup, with a variation, names and a
/// root comment.
const PROBLEM: &str = "(;GM[1]SZ[19]KM[6.5]PB[Black]PW[White]C[root comment]\
    AB[ii][ji][ki][li][ij][lj][ik][jk][kk]AW[jj]\
    ;B[lk];W[kj](;B[aa])(;B[ss]))";

fn rec(wb: &Workbench) -> &GameRecord {
    &wb.document().record
}

fn frame_settings(wall: i32) -> TsumegoFrameSettings {
    TsumegoFrameSettings {
        to_play: TsumegoToPlay::Keep,
        attacker: TsumegoAttacker::Auto,
        ko_threat: TsumegoKoThreat::None,
        wall_distance: wall,
    }
}

fn to_end(wb: &mut Workbench) {
    assert!(wb.navigate(lizzie_core::browse::Navigation::Last));
}

#[test]
fn framing_replaces_the_record_with_the_framed_position_and_its_komi() {
    let tmp = tempfile::tempdir().unwrap();
    let mut wb = opened(tmp.path(), PROBLEM);
    to_end(&mut wb);
    let cursor = wb.cursor();
    let before = rec(&wb).position(cursor);
    let to_play = rec(&wb).node(cursor).to_play;
    let revision = wb.document().revision;
    let expected = build_frame(SIZE, &before.stones, to_play == B, &plain(1))
        .frame
        .unwrap();

    let result = wb
        .edit(EditCommand::FrameTsumego {
            settings: frame_settings(1),
        })
        .unwrap();
    assert_eq!(result, CommandResult::TsumegoFramed { no_room: vec![] });
    let r = rec(&wb);
    assert_eq!(wb.cursor(), ROOT);
    assert!(r.node(ROOT).children.is_empty(), "moves and variations are discarded");
    assert_eq!(r.position(ROOT).stones, expected.stones);
    assert_eq!(r.node(ROOT).to_play == B, expected.black_to_play);
    assert_eq!((r.info.black.as_str(), r.info.white.as_str()), ("", ""));
    assert!(r.node(ROOT).comment.is_empty());
    assert_eq!(r.info.komi, expected.komi);
    assert_eq!(wb.view(false).komi, expected.komi);
    assert!(wb.document().revision > revision);
    assert_eq!(wb.config().tsumego.frame, frame_settings(1));
}

#[test]
fn a_wall_distance_not_above_zero_keeps_the_saved_one() {
    let tmp = tempfile::tempdir().unwrap();
    let mut wb = opened(tmp.path(), PROBLEM);
    let three = wb
        .edit(EditCommand::FrameTsumego {
            settings: frame_settings(3),
        })
        .unwrap();
    assert!(matches!(three, CommandResult::TsumegoFramed { .. }));
    let framed_with_three = rec(&wb).position(ROOT).stones;

    let mut other = opened(tmp.path(), PROBLEM);
    assert_eq!(other.config().tsumego.frame.wall_distance, 3, "the choice is saved");
    other
        .edit(EditCommand::FrameTsumego {
            settings: frame_settings(0),
        })
        .unwrap();
    assert_eq!(other.config().tsumego.frame.wall_distance, 3);
    assert_eq!(rec(&other).position(ROOT).stones, framed_with_three);
}

#[test]
fn a_position_reaching_every_edge_is_left_as_it_is() {
    let tmp = tempfile::tempdir().unwrap();
    let mut wb = opened(tmp.path(), "(;SZ[19]AB[aj][sj][ja]AW[js];B[dd])");
    let revision = wb.document().revision;
    let result = wb
        .edit(EditCommand::FrameTsumego {
            settings: frame_settings(2),
        })
        .unwrap();
    assert_eq!(result, CommandResult::Unchanged);
    assert_eq!(wb.document().revision, revision);
    assert_eq!(rec(&wb).node(ROOT).children.len(), 1);
    assert_eq!(wb.config().tsumego.frame.wall_distance, 2);
}

fn captured(direction: u8, width: u16, height: u16, stones: Vec<u8>) -> EditCommand {
    EditCommand::LoadCapturedProblem {
        problem: CapturedProblem {
            direction,
            width,
            height,
            stones,
        },
    }
}

#[test]
fn a_captured_problem_goes_to_its_corner_of_a_new_board_with_black_to_play() {
    let tmp = tempfile::tempdir().unwrap();
    let mut wb = opened(tmp.path(), PROBLEM);
    let stones = vec![1, 0, 2, 0, 1, 0];
    for (direction, points) in [
        (1, [(0, 0), (2, 0), (1, 1)]),
        (2, [(16, 0), (18, 0), (17, 1)]),
        (3, [(0, 17), (2, 17), (1, 18)]),
        (4, [(16, 17), (18, 17), (17, 18)]),
    ] {
        assert_eq!(
            wb.edit(captured(direction, 3, 2, stones.clone())).unwrap(),
            CommandResult::Done
        );
        let r = rec(&wb);
        assert_eq!(r.size, SIZE);
        assert!(r.node(ROOT).children.is_empty());
        assert_eq!(r.node(ROOT).to_play, B);
        let pos = r.position(ROOT);
        let placed: Vec<_> = points.iter().map(|&(x, y)| pos.at(Point { x, y })).collect();
        assert_eq!(placed, [Some(B), Some(W), Some(B)], "direction {direction}");
        assert_eq!(pos.stones.iter().filter(|s| s.is_some()).count(), 3);
    }
}

#[test]
fn a_capture_larger_than_the_board_opens_a_square_board_of_its_width() {
    let tmp = tempfile::tempdir().unwrap();
    let mut wb = opened(tmp.path(), "(;SZ[9])");
    let mut stones = vec![0; 11 * 4];
    stones[10] = 2;
    wb.edit(captured(1, 11, 4, stones)).unwrap();
    let r = rec(&wb);
    assert_eq!(r.size, BoardSize { width: 11, height: 11 });
    assert_eq!(r.position(ROOT).at(Point { x: 10, y: 0 }), Some(W));
    assert!(
        wb.edit(captured(1, 3, 2, vec![0; 5])).is_err(),
        "rows that do not match the size"
    );
    assert!(wb.edit(captured(5, 3, 2, vec![0; 6])).is_err(), "no such corner");
}

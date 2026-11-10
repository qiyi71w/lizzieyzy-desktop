//! Comparing a helper frame with record positions (Java `SyncSnapshotClassifier`,
//! `SyncSnapshotDiffChecker`, the matching half of `SyncSnapshotRebuildPolicy`).
//!
//! A frame is `width * height` codes in row order: `0` empty, `1`/`3` black, `2`/`4` white, where
//! `3`/`4` also mark the last move. Positions are row-major too (`BoardSize::index`).

use std::fmt::Write;

use super::remote::{RemoteContext, WindowKind};
use crate::record::{BoardSize, Point, Position, Stone};

/// The stone a frame code stands for.
pub(super) fn code_stone(code: u8) -> Option<Stone> {
    match code {
        1 | 3 => Some(Stone::Black),
        2 | 4 => Some(Stone::White),
        _ => None,
    }
}

/// The last-move marker colour of a code.
fn marker_stone(code: u8) -> Option<Stone> {
    match code {
        3 => Some(Stone::Black),
        4 => Some(Stone::White),
        _ => None,
    }
}

pub(super) fn stone_code(stone: Option<Stone>) -> u8 {
    match stone {
        Some(Stone::Black) => 1,
        Some(Stone::White) => 2,
        None => 0,
    }
}

pub(super) fn point_of(size: BoardSize, index: usize) -> Point {
    let w = size.width as usize;
    Point { x: (index % w) as u16, y: (index / w) as u16 }
}

/// `buildSnapshotStones`.
pub(super) fn frame_stones(codes: &[u8]) -> Vec<Option<Stone>> {
    codes.iter().map(|&c| code_stone(c)).collect()
}

/// `snapshotPositionKey`: the stones of a frame, markers folded into their colour; empty for no
/// frame.
pub(super) fn position_key(codes: &[u8]) -> String {
    codes.iter().map(|&c| char::from(b'0' + stone_code(code_stone(c)))).collect()
}

/// The same key for a position.
pub(super) fn stones_key(stones: &[Option<Stone>]) -> String {
    stones.iter().map(|&s| char::from(b'0' + stone_code(s))).collect()
}

/// `SyncSnapshotClassifier.SnapshotDelta`: how a frame differs from a position.
#[derive(Clone, Debug)]
pub(super) struct SnapshotDelta {
    /// The first added stone.
    first_addition: Option<(Point, Stone)>,
    pub additions: u32,
    removed_black: u32,
    removed_white: u32,
    /// No point changed colour (and the sizes matched).
    valid: bool,
    /// At most one marker.
    marker_valid: bool,
    marker: Option<(Point, Stone)>,
}

impl SnapshotDelta {
    fn new() -> SnapshotDelta {
        SnapshotDelta {
            first_addition: None,
            additions: 0,
            removed_black: 0,
            removed_white: 0,
            valid: true,
            marker_valid: true,
            marker: None,
        }
    }

    pub fn removals(&self) -> u32 {
        self.removed_black + self.removed_white
    }

    pub fn changed_stones(&self) -> u32 {
        self.additions + self.removals()
    }

    pub fn has_only_additions(&self) -> bool {
        self.valid && self.removals() == 0
    }

    pub fn has_marker(&self) -> bool {
        self.marker_valid && self.marker.is_some()
    }

    /// The marker, when the frame has exactly one.
    pub fn marker(&self) -> Option<(Point, Stone)> {
        if self.marker_valid {
            self.marker
        } else {
            None
        }
    }

    pub fn single_addition_color(&self) -> Option<Stone> {
        if self.additions == 1 {
            self.first_addition.map(|(_, c)| c)
        } else {
            None
        }
    }

    fn marker_matches_single_addition(&self) -> bool {
        self.marker.is_some() && self.marker == self.first_addition
    }

    /// `allowsIncrementalSync`: nothing changed and no marker, or exactly one marked addition.
    pub fn allows_incremental_sync(&self) -> bool {
        if !self.valid || !self.marker_valid || self.removals() > 0 {
            return false;
        }
        if self.additions == 0 {
            return self.marker.is_none();
        }
        self.additions == 1 && self.marker.is_some() && self.marker_matches_single_addition()
    }

    /// `isSingleMoveCapture`: one addition (matching the marker, if any) that only removed the
    /// other colour.
    fn is_single_move_capture(&self) -> bool {
        let Some((_, color)) = self.first_addition else { return false };
        if !self.valid || !self.marker_valid || self.additions != 1 {
            return false;
        }
        if self.marker.is_some() && !self.marker_matches_single_addition() {
            return false;
        }
        match color {
            Stone::Black => self.removed_black == 0,
            Stone::White => self.removed_white == 0,
        }
    }
}

/// `summarizeDelta`. Stops at the first point whose colour changed, like Java.
pub(super) fn summarize_delta(size: BoardSize, current: &[Option<Stone>], codes: &[u8]) -> SnapshotDelta {
    let mut delta = SnapshotDelta::new();
    if current.len() != size.area() || codes.len() != size.area() {
        delta.valid = false;
        return delta;
    }
    for (index, (&stone, &code)) in current.iter().zip(codes).enumerate() {
        let point = point_of(size, index);
        if let Some(color) = marker_stone(code) {
            if delta.marker.is_some() {
                delta.marker_valid = false;
            } else {
                delta.marker = Some((point, color));
            }
        }
        match (stone, code_stone(code)) {
            (a, b) if a == b => {}
            (None, Some(color)) => {
                delta.additions += 1;
                if delta.first_addition.is_none() {
                    delta.first_addition = Some((point, color));
                }
            }
            (Some(Stone::Black), None) => delta.removed_black += 1,
            (Some(Stone::White), None) => delta.removed_white += 1,
            _ => {
                delta.valid = false;
                return delta;
            }
        }
    }
    delta
}

/// `findSingleMoveCapture`: the one move (with its captures, no suicide) that turns `current`
/// into the frame.
pub(super) fn find_single_move_capture(
    size: BoardSize,
    current: &[Option<Stone>],
    codes: &[u8],
) -> Option<(Point, Stone)> {
    let delta = summarize_delta(size, current, codes);
    if !delta.is_single_move_capture() {
        return None;
    }
    let (point, color) = delta.first_addition?;
    if current[size.index(point)].is_some() {
        return None;
    }
    let mut pos = Position { size, stones: current.to_vec(), black_captures: 0, white_captures: 0 };
    if pos.play(color, point) > 0 {
        return None;
    }
    pos.stones.iter().zip(codes).all(|(&s, &c)| s == code_stone(c)).then_some((point, color))
}

/// `isComparable`.
pub(super) fn comparable(size: BoardSize, codes: &[u8], stones: &[Option<Stone>]) -> bool {
    codes.len() == stones.len() && codes.len().is_multiple_of(size.width as usize)
}

/// `SyncSnapshotDiffChecker.hasDiff`: whether a position differs from the frame. `ignore` is the
/// pending local move, whose stone the frame may not show yet.
pub(super) fn has_diff(size: BoardSize, codes: &[u8], stones: &[Option<Stone>], ignore: Option<Point>) -> bool {
    if !comparable(size, codes, stones) {
        return true;
    }
    codes.iter().zip(stones).enumerate().any(|(index, (&code, &stone))| match (code_stone(code), stone) {
        (None, Some(_)) => ignore != Some(point_of(size, index)),
        (Some(a), b) => b != Some(a),
        (None, None) => false,
    })
}

/// `SyncSnapshotRebuildPolicy.matchesStones`: in a Fox frame a marker only proves the point is
/// occupied (its colour can jitter).
pub(super) fn matches_stones(size: BoardSize, stones: &[Option<Stone>], codes: &[u8], fox: bool) -> bool {
    if stones.len() != codes.len() || !codes.len().is_multiple_of(size.width as usize) {
        return false;
    }
    codes.iter().zip(stones).all(|(&code, &stone)| {
        if fox && marker_stone(code).is_some() {
            stone.is_some()
        } else {
            code_stone(code) == stone
        }
    })
}

/// `findSnapshotMarker`: `Err` for more than one marker.
pub(super) fn snapshot_marker(size: BoardSize, codes: &[u8]) -> Result<Option<(Point, Stone)>, ()> {
    let mut marker = None;
    for (index, &code) in codes.iter().enumerate() {
        if let Some(color) = marker_stone(code) {
            if marker.is_some() {
                return Err(());
            }
            marker = Some((point_of(size, index), color));
        }
    }
    Ok(marker)
}

/// `buildConflictKey`: what makes two conflicting frames "the same conflict". A Fox marker counts
/// as the colour its move number implies, so marker jitter does not open a new conflict.
pub(super) fn conflict_key(codes: &[u8], remote: &RemoteContext) -> String {
    let fox = remote.recovery_move_number();
    let mut key = String::with_capacity(codes.len() + 64);
    for &code in codes {
        let value = match (marker_stone(code), fox) {
            (Some(_), Some(n)) => {
                if n % 2 == 0 {
                    2
                } else {
                    1
                }
            }
            _ => stone_code(code_stone(code)),
        };
        key.push(char::from(b'0' + value));
    }
    let _ = write!(key, "|{:?}|{:?}", remote.platform, remote.window);
    if let Some(n) = fox {
        let _ = write!(key, "|{n}");
    }
    match remote.window {
        WindowKind::LiveRoom => {
            let _ = write!(key, "|{}", remote.room_token.as_deref().unwrap_or(""));
        }
        WindowKind::RecordView => {
            let _ = write!(
                key,
                "|{}|{}",
                remote.record_total_move.unwrap_or(-1),
                remote.title_fingerprint.as_deref().unwrap_or("")
            );
        }
        WindowKind::Unknown => {}
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::readboard::protocol::SyncPlatform;

    const SIZE: BoardSize = BoardSize { width: 3, height: 3 };

    fn stones(list: &[(u16, u16, Stone)]) -> Vec<Option<Stone>> {
        let mut s = vec![None; SIZE.area()];
        for &(x, y, c) in list {
            s[SIZE.index(Point { x, y })] = Some(c);
        }
        s
    }

    fn codes(list: &[(u16, u16, u8)]) -> Vec<u8> {
        let mut c = vec![0; SIZE.area()];
        for &(x, y, v) in list {
            c[SIZE.index(Point { x, y })] = v;
        }
        c
    }

    const B: Stone = Stone::Black;
    const W: Stone = Stone::White;

    #[test]
    fn one_marked_addition_allows_incremental_sync() {
        // SyncSnapshotClassifierTest: a single marked new stone, or an unchanged unmarked frame.
        let delta = summarize_delta(SIZE, &stones(&[(0, 0, B)]), &codes(&[(0, 0, 1), (1, 0, 4)]));
        assert!(delta.allows_incremental_sync());
        assert_eq!(delta.marker(), Some((Point { x: 1, y: 0 }, W)));
        assert!(summarize_delta(SIZE, &stones(&[(0, 0, B)]), &codes(&[(0, 0, 1)])).allows_incremental_sync());
        // An unchanged frame that still shows a marker is not an increment.
        assert!(!summarize_delta(SIZE, &stones(&[(0, 0, B)]), &codes(&[(0, 0, 3)])).allows_incremental_sync());
        // Two additions, or an unmarked addition, are not.
        assert!(!summarize_delta(SIZE, &stones(&[]), &codes(&[(0, 0, 3), (1, 0, 2)])).allows_incremental_sync());
        assert!(!summarize_delta(SIZE, &stones(&[]), &codes(&[(0, 0, 1)])).allows_incremental_sync());
    }

    #[test]
    fn a_point_changing_colour_invalidates_the_delta() {
        let delta = summarize_delta(SIZE, &stones(&[(0, 0, B)]), &codes(&[(0, 0, 4)]));
        assert!(!delta.allows_incremental_sync());
        assert!(!delta.has_only_additions());
        assert_eq!(find_single_move_capture(SIZE, &stones(&[(0, 0, B)]), &codes(&[(0, 0, 4)])), None);
    }

    #[test]
    fn two_markers_are_no_marker() {
        let delta = summarize_delta(SIZE, &stones(&[]), &codes(&[(0, 0, 3), (1, 0, 4)]));
        assert!(!delta.has_marker());
        assert_eq!(delta.marker(), None);
    }

    #[test]
    fn single_move_capture_is_recognised() {
        // White (0,0) in the corner is captured by black at (0,1).
        let before = stones(&[(0, 0, W), (1, 0, B)]);
        let after = codes(&[(1, 0, 1), (0, 1, 3)]);
        assert_eq!(find_single_move_capture(SIZE, &before, &after), Some((Point { x: 0, y: 1 }, B)));
        // The same frame without the capture having happened is not one move.
        let wrong = codes(&[(0, 0, 2), (1, 0, 1), (0, 1, 3), (2, 2, 1)]);
        assert_eq!(find_single_move_capture(SIZE, &before, &wrong), None);
    }

    #[test]
    fn diff_ignores_only_the_pending_local_stone() {
        // SyncSnapshotDiffCheckerTest
        let local = stones(&[(0, 0, B)]);
        let frame = codes(&[]);
        assert!(has_diff(SIZE, &frame, &local, None));
        assert!(!has_diff(SIZE, &frame, &local, Some(Point { x: 0, y: 0 })));
        assert!(has_diff(SIZE, &codes(&[(0, 0, 4)]), &local, Some(Point { x: 0, y: 0 })));
        assert!(has_diff(SIZE, &frame[..6], &local, None));
    }

    #[test]
    fn fox_marker_jitter_keeps_one_conflict_key() {
        // ReadBoardSyncDecisionTest.foxConflictAcrossMarkerPresenceJitterOnlyHoldsOnceBeforeRebuild
        let fox = RemoteContext::generic(false)
            .with_platform(SyncPlatform::Fox)
            .with_room_token("43581号".into())
            .with_live_title_move(Some(2))
            .with_fox_move_number(Some(2));
        let marked = codes(&[(2, 2, 1), (1, 1, 4)]);
        let unmarked = codes(&[(2, 2, 1), (1, 1, 2)]);
        assert_eq!(conflict_key(&marked, &fox), conflict_key(&unmarked, &fox));
        let generic = RemoteContext::generic(false);
        assert_eq!(conflict_key(&marked, &generic), conflict_key(&unmarked, &generic));
        assert_ne!(conflict_key(&marked, &generic), conflict_key(&codes(&[(2, 2, 1)]), &generic));
    }
}

//! Record file reading: byte decoding, format dispatch and the shared import rules.
//!
//! Reading only produces a candidate record; adopting it is the browsing flow's decision.

pub mod gib;
pub mod sgf;

use crate::record::{GameRecord, NodeKind, RecordFormat, Stone};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadError {
    /// The file could not be read.
    Io(String),
    /// The file was read but holds no usable record.
    Invalid(String),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Io(m) => write!(f, "read failed: {m}"),
            ReadError::Invalid(m) => write!(f, "invalid record: {m}"),
        }
    }
}

/// Java dispatches on the path alone: `.gib` (any case) is GIB, everything else is read as SGF
/// (`LizzieFrame.loadFile`, `LizzieFrame.java:5156`).
pub fn format_for_path(path: &str) -> RecordFormat {
    if path.to_lowercase().ends_with(".gib") {
        RecordFormat::Gib
    } else {
        RecordFormat::Sgf
    }
}

/// Decodes record bytes like Java `EncodingDetector` + reader: a detected Windows-1252 is read
/// as GB18030, undetectable text as UTF-8 (`SGFParser.java:75-81`, `EncodingDetector.java`).
/// The detector is chardetng rather than juniversalchardet; both are Mozilla detectors.
pub fn decode(bytes: &[u8]) -> String {
    if let Some((enc, bom_len)) = encoding_rs::Encoding::for_bom(bytes) {
        return enc.decode_without_bom_handling(&bytes[bom_len..]).0.into_owned();
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    let mut detector = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    detector.feed(bytes, true);
    let mut enc = detector.guess(None, chardetng::Utf8Detection::Deny);
    if enc == encoding_rs::WINDOWS_1252 {
        enc = encoding_rs::GB18030;
    }
    enc.decode_without_bom_handling(bytes).0.into_owned()
}

/// Parses decoded text of the given format into a candidate record.
pub fn parse(format: RecordFormat, text: &str) -> Result<GameRecord, ReadError> {
    if text.is_empty() {
        return Err(ReadError::Invalid("empty file".into()));
    }
    match format {
        RecordFormat::Sgf => sgf::read(text).map_err(|e| ReadError::Invalid(e.0)),
        RecordFormat::Gib => gib::read(text).map_err(ReadError::Invalid),
    }
}

/// Reads a record file from disk into a candidate record.
pub fn read_file(path: &std::path::Path) -> Result<(RecordFormat, GameRecord), ReadError> {
    let bytes = std::fs::read(path).map_err(|e| ReadError::Io(e.to_string()))?;
    let format = format_for_path(&path.to_string_lossy());
    let record = parse(format, &decode(&bytes))?;
    Ok((format, record))
}

/// Determines the side to play of every node (`SNAPSHOT_NODE_KIND.md`, "SGF 初始行棋方统一规则").
///
/// Initial snapshots (before the first real action on their path) use, in order: the explicit
/// `PL` in force, the colour of the first real action on the main line from that snapshot
/// (stopping at setup stones or valid `PL`), a completed handicap (consistent `HA >= 2` on a
/// setup-only path without `AW`/`AE`, matching the black stone count) giving White, else Black.
/// Later snapshots keep an explicit `PL` or their parent's side; actions give the opponent.
pub(crate) fn resolve_turns(rec: &mut GameRecord) {
    let count = rec.node_count() as u32;
    // Node ids grow from parent to child, so one ordered pass sees parents first.
    let mut initial = vec![false; count as usize];
    let mut inherited_pl: Vec<Option<Stone>> = vec![None; count as usize];
    for id in 0..count {
        let node = rec.node(id);
        let parent = node.parent;
        let parent_initial = parent.is_none_or(|p| initial[p as usize]);
        let initial_path = parent_initial && !node.kind.is_action();
        let is_initial = initial_path && matches!(node.kind, NodeKind::Snapshot);
        initial[id as usize] = initial_path;
        let pl_in_force = node
            .explicit_to_play
            .or_else(|| parent.filter(|_| initial_path).and_then(|p| inherited_pl[p as usize]));
        inherited_pl[id as usize] = if initial_path { pl_in_force } else { None };

        let to_play = if let Some(pl) = node.explicit_to_play {
            pl
        } else if is_initial {
            pl_in_force
                .or_else(|| first_main_line_action(rec, id))
                .unwrap_or_else(|| {
                    if completed_handicap(rec, id) {
                        Stone::White
                    } else {
                        Stone::Black
                    }
                })
        } else {
            match node.kind {
                NodeKind::Move { color, .. } | NodeKind::Pass { color } => color.opposite(),
                NodeKind::Snapshot | NodeKind::DummyPass => parent.map_or(Stone::Black, |p| rec.node(p).to_play),
            }
        };
        rec.node_mut(id).to_play = to_play;
    }
}

/// First real main-line action, without crossing setup stones or a valid `PL` boundary.
/// A dummy on the main line ends it (Java `next()` excludes end dummies).
fn first_main_line_action(rec: &GameRecord, from: u32) -> Option<Stone> {
    let mut next = rec.main_child(from);
    while let Some(id) = next {
        let node = rec.node(id);
        if node.explicit_to_play.is_some()
            || node
                .properties
                .iter()
                .any(|p| matches!(p.id.as_str(), "AB" | "AW" | "AE"))
            || !node.setup.is_empty()
        {
            return None;
        }
        match node.kind {
            NodeKind::Move { color, .. } | NodeKind::Pass { color } => return Some(color),
            NodeKind::DummyPass => return None,
            NodeKind::Snapshot => next = rec.main_child(id),
        }
    }
    None
}

fn completed_handicap(rec: &GameRecord, id: u32) -> bool {
    let mut ha = rec.info.handicap as usize;
    let mut ancestor = Some(id);
    while let Some(id) = ancestor {
        let node = rec.node(id);
        if node.kind.is_action()
            || !node.setup.add_white.is_empty()
            || !node.setup.clear.is_empty()
            || node.properties.iter().any(|p| matches!(p.id.as_str(), "AW" | "AE"))
        {
            return false;
        }
        for declaration in node.properties.iter().filter(|p| p.id == "HA") {
            let value = declaration
                .values
                .first()
                .and_then(|v| v.trim().parse::<i32>().ok())
                .unwrap_or(0);
            if value < 2 || (ha != 0 && ha != value as usize) {
                return false;
            }
            ha = value as usize;
        }
        ancestor = node.parent;
    }
    if ha < 2 || ha >= rec.size.area() {
        return false;
    }
    let pos = rec.position(id);
    let black = pos.stones.iter().filter(|s| **s == Some(Stone::Black)).count();
    let white = pos.stones.iter().filter(|s| **s == Some(Stone::White)).count();
    white == 0 && black == ha
}

//! Java 存档与读档 slots for the explicit import (`save/save`, `LizzieFrame.getTempGameList`).
//!
//! Java keeps the list in `<work dir>/save/save` (JSON `{"save": {...}}`): manual slot `n` is
//! `game{n}.sgf` with its main-board picture `game{n}.bmp`, the automatic slot `autoGame1.*`
//! while `save-auto-game-index1 > 0`. Java writes manual slots relative to its current
//! directory, which is the work directory of normal installs; a slot not found there is looked
//! for under the chosen program folder.
//!
//! An automatic slot still marked by an unclean session (`save-auto-game-index2 == -5`, which
//! Java promotes at its next start) is taken from `autoGame2` with its own time and position.
//!
//! Every record is read before it is offered; a missing or unreadable one is reported and left
//! out. Nothing in the Java folders is written.

use std::path::PathBuf;

use serde_json::{Map, Value};

use super::{read_json, JavaSource, JsonError, Reader};
use crate::api::{ImportReason, ImportSlotView, ImportStatus};
use crate::formats::{self, picture};
use crate::record::{GameRecord, NodeId, NodeKind, Point, RecordFormat, Stone, ROOT};
use crate::recovery::{child_path, SlotMeta};

/// One Java slot as read.
#[derive(Clone, Debug, PartialEq)]
pub struct JavaSlot {
    pub view: ImportSlotView,
    /// The record file's bytes, for a slot that can be imported.
    pub record: Option<Vec<u8>>,
    /// The picture as PNG, when Java's BMP could be read.
    pub png: Option<Vec<u8>>,
    /// Where the record was (child indexes from the root), resolved from Java's move list.
    pub path: Vec<u32>,
}

impl JavaSlot {
    pub fn importable(&self) -> bool {
        self.record.is_some()
    }

    /// The slot as this product keeps it; Java's automatic slot takes `auto_name`.
    pub fn meta(&self, auto_name: &str) -> SlotMeta {
        SlotMeta {
            name: if self.view.auto {
                auto_name.to_string()
            } else {
                self.view.name.clone()
            },
            time: self.view.time.clone(),
            move_number: self.view.move_number,
            path: self.path.clone(),
            thumbnail: false,
        }
    }
}

/// The views as the report shows them: slots not chosen are left out.
pub fn after(slots: &[JavaSlot], chosen: bool) -> Vec<ImportSlotView> {
    slots
        .iter()
        .map(|s| {
            let mut v = s.view.clone();
            if !chosen && v.status == ImportStatus::Imported {
                v.status = ImportStatus::NotMigrated;
                v.reason = Some(ImportReason::NotChosen);
            }
            v
        })
        .collect()
}

struct Entry {
    file: String,
    auto: bool,
    name: String,
    time: String,
    move_number: u32,
    moves: String,
}

pub(super) fn read(source: &JavaSource, r: &mut Reader) -> Vec<JavaSlot> {
    let list = source.config_dir.join("save").join("save");
    if !list.is_file() {
        return Vec::new();
    }
    let save = match read_json(&list) {
        Ok(Value::Object(m)) => match m.get("save") {
            Some(Value::Object(s)) => s.clone(),
            _ => {
                r.unreadable("file:save/save", "no \"save\" object".into());
                return Vec::new();
            }
        },
        Ok(_) => {
            r.unreadable("file:save/save", "not a JSON object".into());
            return Vec::new();
        }
        Err(JsonError::Io(d) | JsonError::Syntax(d)) => {
            r.unreadable("file:save/save", d);
            return Vec::new();
        }
    };
    // Java saved and replays its slots under its own `ui.no-capture` (`BoardHistoryList.place`).
    let no_capture = r.mapped.no_capture.unwrap_or(false);
    entries(&save)
        .into_iter()
        .map(|e| read_slot(source, e, no_capture))
        .collect()
}

fn entries(save: &Map<String, Value>) -> Vec<Entry> {
    let text = |key: &str| save.get(key).and_then(Value::as_str).unwrap_or("").to_string();
    let int = |key: &str, default: i64| save.get(key).and_then(Value::as_i64).unwrap_or(default);
    let mut out = Vec::new();
    // `getTempGameList`: the automatic slot first. An unclean session's periodic save is what
    // Java shows after its next start.
    let unclean = int("save-auto-game-index2", -1) == -5;
    let auto = if unclean { 2 } else { 1 };
    if unclean || int("save-auto-game-index1", -1) > 0 {
        out.push(Entry {
            file: format!("autoGame{auto}"),
            auto: true,
            name: String::new(),
            time: text(&format!("save-auto-game-time{auto}")),
            move_number: int(&format!("save-auto-game-move-number{auto}"), 0).max(0) as u32,
            moves: text(&format!("save-auto-game-move-list{auto}")),
        });
    }
    let array = |key: &str| save.get(key).and_then(Value::as_array).cloned().unwrap_or_default();
    let (index, names, times, numbers, moves) = (
        array("save-game-index"),
        array("save-game-name"),
        array("save-game-time"),
        array("save-game-move-number"),
        array("save-game-move-list"),
    );
    for (i, n) in index.iter().enumerate() {
        let n = n.as_i64().unwrap_or(i as i64 + 1);
        let at = |list: &Vec<Value>| list.get(i).and_then(Value::as_str).unwrap_or("").to_string();
        out.push(Entry {
            file: format!("game{n}"),
            auto: false,
            name: at(&names),
            time: at(&times),
            move_number: numbers.get(i).and_then(Value::as_i64).unwrap_or(0).max(0) as u32,
            moves: at(&moves),
        });
    }
    out
}

/// `save/<name>` in the Java folder, else under the program folder.
fn find(source: &JavaSource, name: &str) -> Option<PathBuf> {
    std::iter::once(&source.config_dir)
        .chain(source.program_dir.as_ref())
        .map(|d| d.join("save").join(name))
        .find(|p| p.is_file())
}

fn read_slot(source: &JavaSource, e: Entry, no_capture: bool) -> JavaSlot {
    let mut view = ImportSlotView {
        key: format!("save:{}", e.file),
        auto: e.auto,
        name: e.name,
        time: e.time,
        move_number: e.move_number,
        status: ImportStatus::Imported,
        reason: None,
        detail: None,
    };
    let unreadable = |mut view: ImportSlotView, detail: String| {
        view.status = ImportStatus::Incompatible;
        view.reason = Some(ImportReason::Unreadable);
        view.detail = Some(detail);
        JavaSlot {
            view,
            record: None,
            png: None,
            path: Vec::new(),
        }
    };
    let sgf_name = format!("{}.sgf", e.file);
    let Some(sgf_path) = find(source, &sgf_name) else {
        return unreadable(view, format!("save/{sgf_name} not found"));
    };
    let bytes = match std::fs::read(&sgf_path) {
        Ok(b) => b,
        Err(err) => return unreadable(view, format!("{}: {err}", sgf_path.display())),
    };
    let record = match formats::parse(RecordFormat::Sgf, &formats::decode(&bytes), no_capture) {
        Ok(r) => r,
        Err(err) => return unreadable(view, format!("save/{sgf_name}: {err}")),
    };
    let node = position(&record, &e.moves, e.move_number);
    view.move_number = record.node(node).move_number;
    let png = find(source, &format!("{}.bmp", e.file))
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| picture::bmp_to_png(&b).ok());
    JavaSlot {
        view,
        record: Some(bytes),
        png,
        path: child_path(&record, node),
    }
}

fn read_move(text: &str) -> Option<(Option<Point>, Stone)> {
    let mut parts = text.split(',');
    let x: i64 = parts.next()?.trim().parse().ok()?;
    let y: i64 = parts.next()?.trim().parse().ok()?;
    let color = match parts.next()?.trim() {
        "b" => Stone::Black,
        "w" => Stone::White,
        _ => return None,
    };
    if x < 0 {
        return Some((None, color));
    }
    Some((
        Some(Point {
            x: u16::try_from(x).ok()?,
            y: u16::try_from(y).ok()?,
        }),
        color,
    ))
}

/// Java `playList(moves)` after loading (`moveListToString` is newest first; the leading start
/// stones of a setup position are already on the board), else `goToMoveNumber`.
fn position(rec: &GameRecord, moves: &str, move_number: u32) -> NodeId {
    if moves.is_empty() {
        let mut id = ROOT;
        while rec.node(id).move_number < move_number {
            match rec.node(id).children.first() {
                Some(&c) => id = c,
                None => break,
            }
        }
        return id;
    }
    let list: Vec<(Option<Point>, Stone)> = moves.split('_').rev().filter_map(read_move).collect();
    let mut id = ROOT;
    for (point, color) in list {
        let child = rec
            .node(id)
            .children
            .iter()
            .copied()
            .find(|&c| match (rec.node(c).kind, point) {
                (NodeKind::Move { color: k, point: p }, Some(q)) => k == color && p == q,
                (NodeKind::Pass { color: k }, None) => k == color,
                _ => false,
            });
        match child {
            Some(c) => id = c,
            None if id == ROOT
                && point.is_some_and(|p| rec.size.contains(p) && rec.position(ROOT).at(p) == Some(color)) => {}
            None => break,
        }
    }
    id
}

//! 存档与读档 (Java `LizzieFrame.showTempGamePanel`): saved slots of the current record, the
//! automatic save written on exit and every 60 seconds, and their Java import.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{ImportReason, ImportStatus};
use crate::settings::RecoverySettings;

/// A slot of the panel: the automatic one (自动存档) or manual slot `index` (存档N, from 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum SlotRef {
    Auto,
    Manual { index: u32 },
}

/// Pictures travel as base64 PNG of the panel as shown (main board for thumbnails).
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum RecoveryCommand {
    /// 新建存档: a slot after the last one. `name` is the typed name, else the record's save name
    /// (`GameInfo.getSaveFileName`).
    NewSlot { name: String, thumbnail: String },
    /// 保存 on manual slot `index`, after "存档已存在,是否覆盖?": the current record replaces it and
    /// the slot takes `name`, the record's save name (`saveTempGame(index, getSaveFileName())`).
    OverwriteSlot {
        index: u32,
        name: String,
        thumbnail: String,
    },
    /// 重命名.
    RenameSlot { index: u32, name: String },
    /// 加载 or a click on the thumbnail: opens the slot's record like a file (a failure keeps the
    /// current record), then goes to the position it was saved at. Answers `OpenStarted`.
    LoadSlot { slot: SlotRef },
    /// 删除: a manual slot is removed and the later ones move up; the automatic one disappears.
    DeleteSlot { slot: SlotRef },
    /// 全部删除, after confirmation: every manual slot (the automatic one stays).
    DeleteAllSlots,
    /// The panel's check boxes.
    SetSettings { settings: RecoverySettings },
    /// The periodic automatic save (every 60 seconds while 自动存档 is on): a crash leaves it
    /// to be recovered at the next start.
    AutoSave { thumbnail: String },
    /// The exit automatic save, answering the exit request event; `None` when the picture could
    /// not be taken. The window closes afterwards.
    ExitAutoSave { thumbnail: Option<String> },
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct SlotView {
    pub slot: SlotRef,
    /// Empty for the automatic slot (the UI shows `LizzieFrame.saveAndLoad.exitRecord`).
    pub name: String,
    /// Local time it was written, `yyyy-MM-dd HH:mm:ss`.
    pub time: String,
    pub move_number: u32,
    pub has_thumbnail: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct RecoveryView {
    pub settings: RecoverySettings,
    /// The automatic slot, first on the panel when present.
    pub auto: Option<SlotView>,
    pub slots: Vec<SlotView>,
    /// Changes whenever a slot or picture changes (thumbnail cache key).
    pub revision: u64,
}

/// One Java slot in the import preview and report (`save/save`, `save/gameN.sgf`,
/// `save/autoGameN.sgf`).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ImportSlotView {
    /// `save:game2`, `save:autoGame1`.
    pub key: String,
    /// Java's automatic slot; it becomes a manual slot here, so this product's own automatic save
    /// is kept.
    pub auto: bool,
    pub name: String,
    pub time: String,
    pub move_number: u32,
    pub status: ImportStatus,
    /// `Unreadable` (record missing or not readable), `NotChosen`.
    pub reason: Option<ImportReason>,
    pub detail: Option<String>,
}

/// The user chose to import the Java slots ([`super::Command::ApplyJavaImport`]).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub struct RecoveryImport {
    /// Name for Java's automatic slot (the UI's `LizzieFrame.saveAndLoad.exitRecord`).
    pub auto_slot_name: String,
}

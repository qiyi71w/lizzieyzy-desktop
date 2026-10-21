//! Record and picture exports of Java's 文件 > 更多保存 and the clipboard copies.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The record exports of 更多保存 (`SgfSaveCoordinator.Mode`). Each writes through the save chooser
/// and the serial writer like "另存为"; the analysis (`LZ`, `LZ2`) and this product's `AP` are left
/// out of all three.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ExportKind {
    /// 保存纯净棋谱 (Ctrl+Shift+S, `RAW`): the whole tree without move comments. The record then
    /// belongs to the written file, like Java's `curFile`; it stays unsaved because that file does
    /// not hold its comments and analysis.
    Raw,
    /// 保存纯净棋谱(带评论) (`RAW_COMMENT`): the whole tree with comments; an analysed node's comment
    /// carries its win-rate summary (`SGFParser.formatComment`).
    RawWithComments,
    /// 保存纯净分支 (Ctrl+Alt+S, `CURRENT_BRANCH`): root to the current node, then its first
    /// children to the end, as one line; without comments and without the root's comment and
    /// analysis.
    Branch,
}

/// Words of the win-rate summary ([`ExportKind::RawWithComments`]) in the UI language: Java
/// `SGFParse.black`, `.white`, `.winrate`, `.leadJustScore`, `.leadWithKomi`, `.stdev`,
/// `.playouts`, `.komi`, `.pda`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CommentLabels {
    pub black: String,
    pub white: String,
    pub winrate: String,
    /// 目差 (the lead after komi).
    pub lead: String,
    /// 盘面 (`show-katago-score-lead-with-komi`).
    pub lead_with_komi: String,
    pub stdev: String,
    pub playouts: String,
    pub komi: String,
    pub pda: String,
}

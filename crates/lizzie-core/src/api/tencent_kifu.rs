//! 腾讯棋谱 window (`TencentKifuDownload`): its operations and its part of the view.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{KifuNotice, KifuPage};
use crate::settings::{KifuAfterGet, TencentRecentSearch};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum TencentKifuCommand {
    /// The window is opened or brought forward (`openTencentKifu`): the analysis engine preload
    /// for browsing records.
    Open,
    /// 搜索, Enter in the nickname / UID field, or a recent search. Refused with the Java message
    /// key when the text is missing, or while a search or a record download runs.
    Search { text: String },
    /// |< < > >| over the list; the last pages fetch more games.
    Page { to: KifuPage },
    /// 打开 or a double click on a listed game; ignored while a record downloads.
    OpenGame { chessid: String },
    /// 打开棋谱后此窗口.
    SetAfterGet { action: KifuAfterGet },
    /// 清空历史, after its confirmation.
    ClearRecent,
}

/// What the window shows; the core owns the search and its list.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct TencentKifuView {
    /// Starts each search (and the window's first state): the nickname / UID field shows
    /// `last_name` again then.
    pub search: u64,
    pub last_name: String,
    /// 打开棋谱后此窗口: the window's own choice, else 野狐棋谱's.
    pub after_get: KifuAfterGet,
    /// 近期搜索, newest first.
    pub recent: Vec<TencentRecentSearch>,
    /// 当前腾讯查询; empty: 等待搜索.
    pub query: String,
    /// The table's rows (the shown page). A new search keeps the previous rows until its games
    /// arrive.
    pub rows: Vec<TencentKifuRow>,
    /// A list exists: the page buttons work.
    pub listed: bool,
    /// The page label `page/pages`, with `...` while more games may exist.
    pub page: u32,
    pub pages: u32,
    pub complete: bool,
    /// The progress overlay over the window (blocks the window's mouse input).
    pub busy: Option<TencentKifuBusy>,
    /// The table is disabled while a record downloads and opens.
    pub loading: bool,
    /// The latest message box, by increasing id.
    pub notice: Option<KifuNotice>,
    /// Counts records opened from this window: after each, 打开棋谱后此窗口 applies and the main
    /// window comes forward.
    pub opened: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
pub struct TencentKifuRow {
    pub index: u32,
    /// `starttime`; when empty, `start_seconds` (Unix seconds, 0 none) in local time.
    pub time: String,
    pub start_seconds: i64,
    pub black_name: Option<String>,
    pub black_uid: String,
    pub black_rank: i64,
    pub white_name: Option<String>,
    pub white_uid: String,
    pub white_rank: i64,
    pub winner: i64,
    pub point: i64,
    pub rule: i64,
    pub moves: i64,
    /// The searched player won (red text).
    pub won: bool,
    pub chessid: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum TencentKifuBusy {
    /// TencentKifuDownload.searching with the searched text.
    Searching { text: String },
    /// TencentKifuDownload.loadingMore.
    LoadingMore,
    /// KifuLoad.tencentDownloading.
    Downloading,
    /// KifuLoad.parsing: the record is being opened.
    Opening,
}

//! 野狐棋谱 window (`FoxKifuDownload`): its operations and its part of the view.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{KifuNotice, KifuPage};
use crate::settings::{FoxRecentSearch, KifuAfterGet};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum FoxKifuCommand {
    /// The window is opened or brought forward (`openFoxReq`): the analysis engine preload for
    /// browsing records.
    Open,
    /// 搜索, Enter in the account field, or a recent search. `by_uid` is the user's choice (or a
    /// saved search's UID), never guessed from the text; `nickname_hint` labels a UID search
    /// until its games reveal the name. Refused with the Java message key when the text is
    /// missing or not a UID, or while a search or a record download runs.
    Search {
        text: String,
        by_uid: bool,
        nickname_hint: String,
    },
    /// |< < > >| over the list (25 games a page); the last pages fetch more games.
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
pub struct FoxKifuView {
    /// Starts each search (and the window's first state): the account field and 查询方式 show
    /// `last_name` / `last_query_by_uid` again then.
    pub search: u64,
    pub last_name: String,
    pub last_query_by_uid: bool,
    pub after_get: KifuAfterGet,
    /// 近期搜索, newest first.
    pub recent: Vec<FoxRecentSearch>,
    /// 当前野狐账号; both empty: 等待搜索.
    pub nickname: String,
    pub uid: String,
    /// The table's rows (the shown page). A new search keeps the previous rows until its games
    /// arrive.
    pub rows: Vec<FoxKifuRow>,
    /// A list exists: the page buttons work.
    pub listed: bool,
    /// The page label `page/pages`, with `...` while more games may exist.
    pub page: u32,
    pub pages: u32,
    pub complete: bool,
    /// The progress overlay over the window (blocks the window's mouse input).
    pub busy: Option<FoxKifuBusy>,
    /// The table is disabled while a record downloads and opens.
    pub loading: bool,
    /// The latest message box, by increasing id.
    pub notice: Option<KifuNotice>,
    /// Counts records opened from this window: after each, 打开棋谱后此窗口 applies and the main
    /// window comes forward.
    pub opened: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
pub struct FoxKifuRow {
    pub index: u32,
    pub time: String,
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
    /// The searched account won (red text).
    pub won: bool,
    pub chessid: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum FoxKifuBusy {
    /// KifuLoad.foxSearching with the searched text.
    Searching { text: String },
    /// KifuLoad.foxDownloading.
    Downloading,
    /// KifuLoad.parsing: the record is being opened.
    Opening,
}

//! OGS 棋谱 window (Java `OgsKifuDownload`, `OgsKifuBrowser`): its operations and its part of the
//! view.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::KifuPage;
use crate::ogs_kifu::OgsFailure;
use crate::settings::KifuAfterGet;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum OgsKifuCommand {
    /// The window is opened or brought forward (`Menu.openKifu`): the analysis engine preload for
    /// browsing records.
    Open,
    /// 搜索, Enter in the username / player ID field, or a recent search. Ignored while a request
    /// runs; a blank text asks for a player.
    Search { text: String },
    /// |< < > >| over the searched player's games; ignored without a player, while a request runs,
    /// or when the page is the shown one.
    Page { to: KifuPage },
    /// 打开, a double click or Enter on a listed game; ignored while a request runs.
    OpenGame { id: u64 },
    /// 打开棋谱后此窗口.
    SetAfterGet { action: KifuAfterGet },
    /// 清空历史.
    ClearRecent,
    /// The window was closed: its request is cancelled (a reply arriving later changes nothing)
    /// and the next opening starts without a player or list, as Java's new window does. The recent
    /// searches stay.
    Close,
}

/// What the window shows; the core owns the search, the list and the request.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct OgsKifuView {
    /// 打开棋谱后此窗口: the window's own choice, else 野狐棋谱's.
    pub after_get: KifuAfterGet,
    /// 近期搜索, newest first.
    pub recent: Vec<String>,
    /// The line under the search rows.
    pub label: OgsKifuLabel,
    /// The shown page's games.
    pub rows: Vec<OgsKifuRow>,
    /// A player was found: the page buttons work.
    pub listed: bool,
    pub page: u32,
    pub pages: u32,
    /// Bumped whenever a page is shown: the window then selects its first game.
    pub shown: u64,
    /// A request runs or a downloaded record opens: the controls wait.
    pub busy: bool,
    /// The status line.
    pub status: Option<OgsKifuStatus>,
    /// Counts records opened from this window: after each, 打开棋谱后此窗口 applies and the main
    /// window comes forward.
    pub opened: u64,
}

/// `playerLabel`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum OgsKifuLabel {
    /// OgsKifuDownload.hint: nothing searched yet.
    Hint,
    /// The text of the search that runs or found no player.
    Searched { text: String },
    /// The found player: `name (id)`.
    Player { name: String, id: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
pub struct OgsKifuRow {
    pub id: u64,
    /// When the game ended, as OGS writes it (ISO-8601); shown in local time.
    pub ended: String,
    pub black: String,
    pub white: String,
    pub result: String,
    pub size: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum OgsKifuStatus {
    /// 搜索 with a blank field: OgsKifuDownload.player.
    NeedPlayer,
    /// KifuLoad.sharedDownloading.
    Downloading,
    /// KifuLoad.parsing: the downloaded record is being opened.
    Opening,
    /// OgsKifuDownload.noPlayer.
    NoPlayer,
    /// OgsKifuDownload.empty: the player has no completed public games.
    Empty,
    /// A search or page failed (`record` false), or a game could not be downloaded or opened.
    Failed { failure: OgsFailure, record: bool },
}

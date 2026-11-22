//! 在线棋谱 and 弈客 commands and projection (`OnlineDialog`, `YikeLiveDialog`, the 弈客 web
//! window and the 同步 menu's live options).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use crate::online::yike::{LiveCategory, LiveGameView, LiveWinrate};
pub use crate::online::OnlineText;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum OnlineCommand {
    /// 打开在线链接 > 确定 (`applyChange`): follow `url`; `refresh_seconds` is the 刷新 field
    /// (blank or ≤0 is 10). A plain link is read once and opened as a record.
    Open { url: String, refresh_seconds: i64 },
    /// 中断, opening 打开在线链接 again, the web window's 停止同步 without a page
    /// (`stopSync`): the follow ends; the record stays.
    Stop,
    /// 弈客直播中心 同步 (`syncSelectedGame` → `LizzieFrame.syncOnline`): follow a listed room.
    FollowLive { url: String },
    /// 弈客直播中心 刷新 / 分类 (reset) and 更多 (`loadGames`).
    LoadLiveList { category: LiveCategory, more: bool },
    /// 同步 > 有新的一手时总是跳转到最新一手 (`always-gotolast-onlive`).
    SetAlwaysGotoLast { enabled: bool },
    /// 同步 > 允许弈客直播/大厅进入对局室 (`open-html-onlive`).
    SetOpenHtmlOnLive { enabled: bool },
    /// The 弈客 web window's 加载 / Enter (`onLoad` → `startYikeSync(addr, true)`): page
    /// addresses are followed from now on, starting with `url`.
    BrowserLoad { url: String },
    /// The web window's 停止同步 (`stopYikeBrowserSync(true)`).
    BrowserStopSync,
}

/// What the 弈客 web page reports through its bridge. The platform admits it only from the
/// 弈客 page webview at a 弈客 address; the follow flow still treats every field as untrusted.
#[derive(Clone, Debug, PartialEq)]
pub enum PageReport {
    /// The page's address changed (load, hash route, history).
    Address { url: String },
    /// The 对局室 poller (`yikeUnite:`) of `room`: `tag` is `ping`, `error` or `resp`; for `resp`
    /// the game-server reply's HTTP status and body, for `error` its message. Only the follow of
    /// that room takes it.
    Unite { room: u64, tag: String, status: u16, body: String, message: String },
}

/// Platform work the follow flow asks for ([`crate::workbench::Workbench::take_online_jobs`]).
#[derive(Clone, Debug, PartialEq)]
pub enum OnlineJob {
    /// A GET through the configured network proxy; the reply goes to `online_http(request, …)`.
    Get { request: u64, url: String, headers: Vec<(String, String)>, timeout_ms: u64 },
    /// Opens WebSocket `url`; events go to `online_socket(conn, …)`. Any earlier connection of
    /// the flow is already closed by a `Close` before this. `proxied`: through the configured
    /// network proxy (huanle), else directly (弈客 socket.io, which Java connects directly).
    Connect { conn: u64, url: String, proxied: bool },
    Send { conn: u64, frame: crate::online::Frame },
    Close { conn: u64 },
    /// Installs the 对局室 poller in the 弈客 page (`installYikeUnitePoller`).
    UnitePoller { room: u64, interval_ms: u64, info_url: String },
    /// `stopYikeUnitePoller`.
    StopUnitePoller,
    /// Reloads the 弈客 page at `url` so the page hooks are in place before its first request
    /// (`onAddressChange` after an automatic follow).
    ReloadPage { url: String },
}

/// The followed source.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct OnlineFollowView {
    pub url: String,
    /// Java type: 1, 2, 5, 6, 7 弈客; 3, 4 huanle.
    pub kind: u8,
    pub yike: bool,
}

/// A status for the 弈客直播中心 status line (`updateYikeSyncStatus`): shown when the dialog's
/// active room is `url` or it follows nothing.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct OnlineStatusView {
    pub seq: u64,
    pub url: String,
    pub text: OnlineText,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct LiveListView {
    pub category: LiveCategory,
    pub loading: bool,
    pub games: Vec<LiveGameView>,
    /// 正在加载弈客直播... / 已加载 N 局 / 加载弈客直播失败: …
    pub status: Option<OnlineText>,
    /// Moves on with every loaded list.
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct OnlineView {
    pub following: Option<OnlineFollowView>,
    pub status: Option<OnlineStatusView>,
    /// `revealYikeLiveSyncStatus`: brings 弈客直播中心 to the front when it moves on.
    pub reveal: u64,
    /// `error(true)` for a link started from 打开在线链接: show 无效的链接 and the dialog again.
    pub dialog_error: u64,
    /// The 网络代理设置 warning: `message` is Java's text; a new `seq` shows it again.
    pub proxy_warning: Option<ProxyWarningView>,
    /// The web window follows page addresses (`yikeBrowserSyncEnabled`).
    pub browser_sync: bool,
    /// The web window title's status (`reportSyncStatus`).
    pub browser_status: Option<OnlineText>,
    pub live: LiveListView,
    pub always_goto_last: bool,
    pub open_html_on_live: bool,
}

/// A proxy that cannot be used, reported by a follow's read (Java `showNetworkProxyConfigWarning`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
pub struct ProxyWarningView {
    pub seq: u64,
    pub message: String,
}

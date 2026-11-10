//! 在线棋谱 / 弈客直播: the one follow of an online source (fixed Java `cb53acc`
//! `gui/OnlineDialog`, `YikeLiveDialog`, `BrowserFrame`, `LizzieFrame.syncOnline` /
//! `syncLiveBoardStat` / `scheduleYikeLiveCurveCompletion`).
//!
//! The workbench is the only writer of the follow: its identity (`session`), the requests and
//! connections it made (replies of an earlier follow are dropped), and every document write. The
//! source owns the main line only ([`DocumentEdit::SyncMainline`]): user branches and comments
//! stay, and browsing follows a new move only on the first sync, when the user was at the
//! previous live end, or with 有新的一手时总是跳转到最新一手. Java's type-6 path replaced the whole
//! history on every move instead (user variations lost, browsing pulled to the end); that
//! overwrite is not reproduced.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{OpenOrigin, Workbench};
use crate::api::*;
use crate::formats;
use crate::online::url::{classify, yike_detail_url, yike_room_key, OnlineSource};
use crate::online::{huanle, rtgame, yike, HttpNeed, HttpReply, OnlineText, SocketEvent, SourceUpdate, Step};
use crate::record::{DocumentEdit, GameRecord, NodeId, NodeKind, RecordFormat, SourceAction, ROOT};

/// The 刷新 field's text when a room is followed from the list or the web window (`txtRefreshTime`
/// defaults to "1", OD 358; `syncOnline` never shows the dialog).
const LIVE_REFRESH_SECONDS: i64 = 1;
/// `scheduleYikeLiveCurveCompletion`: debounce, and the retry while analysis is busy (LF 359-360).
const CURVE_DELAY: Duration = Duration::from_millis(1200);
const CURVE_RETRY: Duration = Duration::from_millis(3000);
/// Java `get()`'s mobile WeChat User-Agent for plain links (OD 1438-1440).
const PLAIN_USER_AGENT: &str = "Mozilla/5.0 (Linux; U; Android 2.3.6; zh-cn; GT-S5660 Build/GINGERBREAD) AppleWebKit/533.1 (KHTML, like Gecko) Version/4.0 Mobile Safari/533.1 MicroMessenger/4.5.255";
/// The 对局室 response cap the page bridge applies (`body.slice(0, 500000)`, BF 1123).
const PAGE_BODY_LIMIT: usize = 500_000;

/// Java's hard-coded Chinese status texts are shown as they are in every language.
fn raw(text: impl Into<String>) -> OnlineText {
    OnlineText { key: "Online.text".into(), args: vec![text.into()] }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Origin {
    /// 打开在线链接 (`fromBrowser = false`): failures show 无效的链接 again.
    Dialog,
    /// 弈客直播中心 or the web window (`syncOnline` → `applyChangeWeb`, `fromBrowser = true`).
    Web,
}

enum Transport {
    /// Type 6: the signed detail endpoint, read now and `interval` after each reply
    /// (`scheduleWithFixedDelay`); `None` once the game is over or the proxy cannot be used.
    Poll { url: String, interval: Duration, due: Option<Instant>, in_flight: bool },
    /// Type 2 and plain links: one read.
    Once { plain: bool },
    Rtgame(Box<rtgame::Session>),
    Huanle(Box<huanle::Session>),
    /// Type 7: the 弈客 page's own requests.
    Unite,
    /// The connection was refused before opening (unusable proxy settings): nothing more is
    /// read, as Java's client never existed.
    Refused,
}

struct Follow {
    session: u64,
    url: String,
    source: OnlineSource,
    origin: Origin,
    transport: Transport,
    /// The open connection and a pending (re)connection.
    conn: Option<u64>,
    connect_due: Option<(Instant, String)>,
    /// The document written to, once the first game was adopted; the open ticket meanwhile.
    doc: Option<u64>,
    adopting: Option<u64>,
    /// Nodes the follow created, with the comment it gave them: only those, untouched, are
    /// dropped when the source takes moves back.
    made: HashMap<NodeId, String>,
    /// The node of the source's last move.
    live_end: Option<NodeId>,
    finished: bool,
    last_mainline: String,
    last_hands: i64,
    /// What the source reported while the first game was still being adopted, in order.
    queued: Vec<SourceUpdate>,
    /// The komi of the game being adopted is the user's (`resolveLiveKomi` with
    /// `changedKomi`); it stays the user's after the adoption.
    keep_komi: bool,
}

#[derive(Clone, Copy)]
enum Purpose {
    List { reset: bool, page: u32 },
    Follow(u64),
    Session(u64, u32),
}

#[derive(Default)]
struct LiveList {
    category: LiveCategory,
    page: u32,
    since: i64,
    request: Option<u64>,
    games: Vec<LiveGameView>,
    status: Option<OnlineText>,
    generation: u64,
}

struct Curve {
    due: Instant,
    started: bool,
    doc: u64,
}

#[derive(Default)]
pub(super) struct OnlineFlow {
    next_id: u64,
    follow: Option<Follow>,
    requests: HashMap<u64, Purpose>,
    jobs: Vec<OnlineJob>,
    status: Option<OnlineStatusView>,
    status_seq: u64,
    reveal: u64,
    dialog_error: u64,
    proxy_warning: Option<ProxyWarningView>,
    browser_sync: bool,
    last_auto_room: Option<String>,
    browser_status: Option<OnlineText>,
    live: LiveList,
    curve: Option<Curve>,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn nonce() -> u32 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.subsec_nanos()) % 100_000_000
}

/// `syncStatusPrefix` (OD 4541-4561): the label, ` 房间 <id> ` for a room.
fn prefix(source: &OnlineSource) -> String {
    let (label, room) = match source {
        OnlineSource::YikeUnite { room, .. } => ("弈客对弈", *room),
        OnlineSource::YikeNewLive { room, .. } => ("弈客直播(新)", *room),
        OnlineSource::YikeLiveRoom { room, .. } => ("弈客直播", *room),
        OnlineSource::YikeLiveBoard { id, .. } => ("弈客直播", id.parse().unwrap_or(0)),
        OnlineSource::YikeGameRoom { room, .. } => ("弈客对局", *room),
        _ => ("同步", 0),
    };
    if room > 0 { format!("{label} 房间 {room} ") } else { format!("{label} ") }
}

/// The source's main line after the root, or `None` when it holds anything but moves and passes
/// (a setup node), which the follow answers by adopting the game again.
fn source_line(record: &GameRecord) -> Option<Vec<SourceAction>> {
    let mut line = Vec::new();
    let mut at = ROOT;
    while let Some(next) = record.main_child(at) {
        let node = record.node(next);
        match node.kind {
            NodeKind::Move { .. } | NodeKind::Pass { .. } => {
                line.push(SourceAction { kind: node.kind, comment: node.comment.clone() })
            }
            NodeKind::DummyPass if node.children.is_empty() => {}
            _ => return None,
        }
        at = next;
    }
    Some(line)
}

impl Workbench {
    pub(super) fn online_view(&self) -> OnlineView {
        let o = &self.online;
        OnlineView {
            following: o.follow.as_ref().map(|f| OnlineFollowView {
                url: f.url.clone(),
                kind: f.source.java_type(),
                yike: f.source.is_yike(),
            }),
            status: o.status.clone(),
            reveal: o.reveal,
            dialog_error: o.dialog_error,
            proxy_warning: o.proxy_warning.clone(),
            browser_sync: o.browser_sync,
            browser_status: o.browser_status.clone(),
            live: LiveListView {
                category: o.live.category,
                loading: o.live.request.is_some(),
                games: o.live.games.clone(),
                status: o.live.status.clone(),
                generation: o.live.generation,
            },
            always_goto_last: self.config.live.always_goto_last,
            open_html_on_live: self.config.live.open_html_on_live,
        }
    }

    pub fn online(&mut self, op: OnlineCommand) -> Result<CommandResult, ApiError> {
        match op {
            OnlineCommand::Open { url, refresh_seconds } => {
                // proc(): a blank or non-positive 刷新 is 10 seconds.
                let refresh = if refresh_seconds > 0 { refresh_seconds } else { 10 };
                self.online_start(&url, Origin::Dialog, refresh);
            }
            OnlineCommand::Stop => self.online_stop(),
            OnlineCommand::FollowLive { url } => self.online_start(&url, Origin::Web, LIVE_REFRESH_SECONDS),
            OnlineCommand::LoadLiveList { category, more } => self.online_load_list(category, more),
            OnlineCommand::SetAlwaysGotoLast { enabled } => {
                self.commit_config(|c| c.live.always_goto_last = enabled)?;
            }
            OnlineCommand::SetOpenHtmlOnLive { enabled } => {
                self.commit_config(|c| c.live.open_html_on_live = enabled)?;
            }
            OnlineCommand::BrowserLoad { url } => self.online_browser_load(&url),
            OnlineCommand::BrowserStopSync => {
                // stopYikeBrowserSync(true) (BF 1001-1024).
                self.online.browser_sync = false;
                self.online.last_auto_room = None;
                if self.online.follow.is_some() {
                    self.online_stop();
                } else {
                    self.online.browser_status = Some(raw("无同步任务"));
                }
            }
        }
        Ok(CommandResult::Done)
    }

    pub fn take_online_jobs(&mut self) -> Vec<OnlineJob> {
        std::mem::take(&mut self.online.jobs)
    }

    pub fn set_yike_browser_placement(&mut self, placement: crate::settings::WindowPlacement) {
        self.persisted.yike_browser = Some(placement);
        let _ = self.save_persisted();
    }

    fn online_id(&mut self) -> u64 {
        self.online.next_id += 1;
        self.online.next_id
    }

    fn online_get(&mut self, purpose: Purpose, url: String, headers: Vec<(String, String)>) {
        let request = self.online_id();
        self.online.requests.insert(request, purpose);
        self.online.jobs.push(OnlineJob::Get { request, url, headers, timeout_ms: yike::HTTP_TIMEOUT_MS });
    }

    /// `updateYikeSyncStatus` (弈客直播中心, Yike follows only) and the web window's status.
    fn online_status(&mut self, text: OnlineText) {
        let Some(f) = &self.online.follow else { return };
        if f.source.is_yike() {
            self.online.status_seq += 1;
            self.online.status = Some(OnlineStatusView { seq: self.online.status_seq, url: f.url.clone(), text });
        }
    }

    // ---------------------------------------------------------------- start and stop

    /// `syncOnline` / `applyChange`: any follow stops first; then the link's source starts.
    fn online_start(&mut self, url: &str, origin: Origin, refresh_seconds: i64) {
        self.online_stop();
        let url = url.trim().to_string();
        let Some(source) = classify(&url) else {
            if origin == Origin::Dialog {
                self.online.dialog_error += 1;
            } else {
                self.online.browser_status = Some(raw("URL 无法识别"));
            }
            return;
        };
        self.online.next_id += 1;
        let session = self.online.next_id;
        let refresh = Duration::from_secs(refresh_seconds.max(1) as u64);
        let (transport, first) = match &source {
            OnlineSource::YikeNewLive { id, .. } => (
                Transport::Poll { url: yike_detail_url(id), interval: refresh, due: Some(Instant::now()), in_flight: false },
                None,
            ),
            OnlineSource::YikeLiveBoard { .. } => (Transport::Once { plain: false }, None),
            OnlineSource::Plain { .. } => (Transport::Once { plain: true }, None),
            OnlineSource::YikeLiveRoom { room, .. } | OnlineSource::YikeGameRoom { room, .. } => {
                let (s, step) = rtgame::Session::new(*room);
                (Transport::Rtgame(Box::new(s)), Some(step))
            }
            OnlineSource::HuanleGame { query } => {
                let (s, step) = huanle::Session::new(query, None, refresh);
                (Transport::Huanle(Box::new(s)), Some(step))
            }
            OnlineSource::HuanleTag { query, request_url } => {
                let (s, step) = huanle::Session::new(query, Some(request_url), refresh);
                (Transport::Huanle(Box::new(s)), Some(step))
            }
            OnlineSource::YikeUnite { .. } => (Transport::Unite, None),
        };
        self.online.follow = Some(Follow {
            session,
            url: url.clone(),
            source: source.clone(),
            origin,
            transport,
            conn: None,
            connect_due: None,
            doc: None,
            adopting: None,
            made: HashMap::new(),
            live_end: None,
            finished: false,
            last_mainline: String::new(),
            last_hands: -1,
            queued: Vec::new(),
            keep_komi: false,
        });
        match &source {
            OnlineSource::YikeLiveBoard { url, .. } => self.online_get(Purpose::Follow(session), url.clone(), Vec::new()),
            OnlineSource::Plain { url } => {
                self.online_get(Purpose::Follow(session), url.clone(), vec![("User-Agent".into(), PLAIN_USER_AGENT.into())])
            }
            OnlineSource::YikeUnite { room, info_url } => {
                // startYikeUnitePolling (OD 4122-4141): every max(刷新, 5) seconds.
                let interval_ms = (refresh_seconds.max(5) as u64) * 1000;
                self.online.jobs.push(OnlineJob::UnitePoller { room: *room, interval_ms, info_url: info_url.clone() });
            }
            _ => {}
        }
        if let Some(step) = first {
            self.online_step(session, step);
        }
        if source.is_yike() {
            // applyChangeWeb: "<prefix>已启动".
            self.online.browser_status = Some(raw(format!("{}已启动", prefix(&source))));
        }
    }

    /// `stopSync` (OD 4460-4504): the follow, its requests, connection and page poller end;
    /// the record stays as it is.
    pub(super) fn online_stop(&mut self) {
        let Some(f) = self.online.follow.take() else { return };
        // A game still waiting for a borrowed engine is not adopted any more.
        if let Some(ticket) = f.adopting {
            self.withdraw_open(ticket);
        }
        self.online.requests.retain(|_, p| match p {
            Purpose::Follow(s) | Purpose::Session(s, _) => *s != f.session,
            Purpose::List { .. } => true,
        });
        if let Some(conn) = f.conn {
            self.online.jobs.push(OnlineJob::Close { conn });
        }
        if matches!(f.transport, Transport::Unite) {
            self.online.jobs.push(OnlineJob::StopUnitePoller);
        }
        self.online.curve = None;
        self.online.browser_status = Some(raw("已停止同步"));
    }

    /// 加载 (`startYikeSync(addr, true)`, BF 965-999): a recognized 弈客 room is followed, and from
    /// then on so are the page's room addresses; anything else is `URL 无法识别`.
    fn online_browser_load(&mut self, url: &str) {
        let url = url.trim();
        let Some(key) = yike_room_key(url) else {
            self.online.browser_status = Some(raw("URL 无法识别"));
            return;
        };
        self.online.browser_sync = true;
        self.online.last_auto_room = Some(key);
        self.online_start(url, Origin::Web, LIVE_REFRESH_SECONDS);
    }

    // ---------------------------------------------------------------- page bridge

    pub fn online_page(&mut self, report: PageReport) -> bool {
        match report {
            PageReport::Address { url } => {
                if !self.online.browser_sync {
                    return false;
                }
                let Some(key) = yike_room_key(&url) else { return false };
                if self.online.last_auto_room.as_deref() == Some(key.as_str()) {
                    return false;
                }
                self.online.last_auto_room = Some(key);
                self.online_start(&url, Origin::Web, LIVE_REFRESH_SECONDS);
                self.online.jobs.push(OnlineJob::ReloadPage { url });
                true
            }
            PageReport::Unite { room, tag, status, body, message } => self.online_unite(room, &tag, status, &body, &message),
        }
    }

    /// `handleYikeUniteSgf` (OD 4144-4228), for the room followed now only.
    fn online_unite(&mut self, room: u64, tag: &str, status: u16, body: &str, message: &str) -> bool {
        let Some(f) = &self.online.follow else { return false };
        if !matches!(f.source, OnlineSource::YikeUnite { room: r, .. } if r == room) {
            return false;
        }
        let session = f.session;
        let prefix = prefix(&f.source);
        match tag {
            "unavailable" => {
                self.online.browser_status = Some(OnlineText::new("Online.uniteNeedsPage"));
                self.online_status(OnlineText::new("Online.uniteNeedsPage"));
                true
            }
            "error" => {
                self.online.browser_status = Some(raw(format!("拉取异常: {}", message.chars().take(200).collect::<String>())));
                true
            }
            "resp" => {
                if status != 200 {
                    self.online.browser_status = Some(raw(format!("game-server 响应 HTTP {status}")));
                    return true;
                }
                let body: String = body.chars().take(PAGE_BODY_LIMIT).collect();
                let data = match serde_json::from_str::<serde_json::Value>(&body) {
                    Ok(root) => root.get("data").cloned().unwrap_or(serde_json::Value::Null),
                    Err(e) => {
                        self.online.browser_status = Some(raw(format!("解析响应失败: {e}")));
                        return true;
                    }
                };
                if !data.is_object() {
                    return false;
                }
                let payload = yike::payload_of(&data);
                let hands = payload.hands.unwrap_or(-1);
                let f = self.online.follow.as_mut().expect("checked");
                if hands >= 0 && hands == f.last_hands {
                    return false;
                }
                let mainline = yike::mainline_only(&payload.sgf);
                if payload.sgf.trim().is_empty() || mainline == f.last_mainline {
                    f.last_hands = hands;
                    return false;
                }
                f.last_hands = hands;
                f.last_mainline = mainline;
                self.online_payload(session, payload, false);
                self.online.browser_status = Some(raw(format!("{prefix}第 {} 手", hands.max(0))));
                true
            }
            _ => false,
        }
    }

    // ---------------------------------------------------------------- the live list

    /// `YikeLiveDialog.loadGames`: a reset reads page 1 / since 0, 更多 the next page; a newer
    /// request supersedes one still loading.
    fn online_load_list(&mut self, category: LiveCategory, more: bool) {
        let list = &mut self.online.live;
        if let Some(old) = list.request.take() {
            self.online.requests.remove(&old);
        }
        let reset = !more;
        list.category = category;
        let page = if reset { 1 } else { list.page + 1 };
        let since = if reset { 0 } else { list.since };
        list.status = Some(OnlineText::new("YikeLiveDialog.loading"));
        let url = yike::list_url(category, page, since);
        let request = self.online_id();
        self.online.requests.insert(request, Purpose::List { reset, page });
        self.online.live.request = Some(request);
        self.online.jobs.push(OnlineJob::Get {
            request,
            url,
            headers: yike::headers(now_ms(), nonce()),
            timeout_ms: yike::HTTP_TIMEOUT_MS,
        });
    }

    fn online_list_reply(&mut self, request: u64, reset: bool, page: u32, reply: HttpReply) {
        if self.online.live.request != Some(request) {
            return;
        }
        self.online.live.request = None;
        let parsed = match reply {
            HttpReply::Ok { status, body } if status < 400 => yike::parse_list(&body),
            HttpReply::Ok { status, body } => Err(format!("Yike request failed: HTTP {status} {body}")),
            // 直播中心 shows only 加载弈客直播失败 (YikeLiveDialog.loadGames), never 网络代理设置.
            HttpReply::Failed { message, .. } => Err(message),
        };
        let list = &mut self.online.live;
        match parsed {
            Ok((since, games)) => {
                if reset {
                    list.games.clear();
                }
                list.games.extend(games);
                list.page = page;
                list.since = since;
                list.generation += 1;
                list.status = Some(OnlineText::with("YikeLiveDialog.loaded", [list.games.len().to_string()]));
            }
            Err(message) => list.status = Some(OnlineText::with("YikeLiveDialog.loadFailed", [message])),
        }
    }

    // ---------------------------------------------------------------- transport input

    pub fn online_http(&mut self, request: u64, reply: HttpReply) -> bool {
        let Some(purpose) = self.online.requests.remove(&request) else { return false };
        match purpose {
            Purpose::List { reset, page } => self.online_list_reply(request, reset, page, reply),
            Purpose::Follow(session) => self.online_follow_reply(session, reply),
            Purpose::Session(session, need) => {
                let step = match self.online.follow.as_mut().filter(|f| f.session == session).map(|f| &mut f.transport) {
                    Some(Transport::Huanle(s)) => s.on_http(need, &reply),
                    _ => return false,
                };
                if let HttpReply::Failed { proxy: true, message } = &reply {
                    self.online_proxy_warning(message);
                }
                self.online_step(session, step);
            }
        }
        true
    }

    /// Java `showNetworkProxyConfigWarning`: every follow read that met a proxy no request can use.
    fn online_proxy_warning(&mut self, message: &str) {
        let seq = self.online.proxy_warning.as_ref().map_or(1, |w| w.seq + 1);
        self.online.proxy_warning = Some(ProxyWarningView { seq, message: message.to_string() });
    }

    pub fn online_socket(&mut self, conn: u64, event: SocketEvent) -> bool {
        let Some(f) = self.online.follow.as_mut().filter(|f| f.conn == Some(conn)) else { return false };
        let session = f.session;
        if let SocketEvent::Refused { message } = event {
            // NetworkProxy refuses unusable settings before connecting (`req` throws): 网络代理设置,
            // and the connection is never made again with them.
            f.conn = None;
            f.connect_due = None;
            f.transport = Transport::Refused;
            self.online_proxy_warning(&message);
            return true;
        }
        if let SocketEvent::Closed { .. } = event {
            f.conn = None;
        }
        let step = match (&mut f.transport, &event) {
            (Transport::Rtgame(s), SocketEvent::Opened) => s.on_open(),
            (Transport::Rtgame(s), SocketEvent::Text(t)) => s.on_text(t),
            (Transport::Rtgame(s), SocketEvent::Closed { error }) => s.on_closed(error.as_deref()),
            (Transport::Huanle(s), SocketEvent::Opened) => s.on_open(),
            (Transport::Huanle(s), SocketEvent::Binary(b)) => s.on_binary(b),
            (Transport::Huanle(s), SocketEvent::Closed { error }) => s.on_closed(error.as_deref()),
            _ => return false,
        };
        self.online_step(session, step);
        true
    }

    /// Performs a session's step for the follow `session`.
    fn online_step(&mut self, session: u64, step: Step) {
        let Some(f) = self.online.follow.as_mut().filter(|f| f.session == session) else { return };
        if step.close {
            if let Some(conn) = f.conn.take() {
                self.online.jobs.push(OnlineJob::Close { conn });
            }
        }
        if let Some(conn) = f.conn {
            for frame in step.send {
                self.online.jobs.push(OnlineJob::Send { conn, frame });
            }
        }
        if let Some(c) = step.connect {
            f.connect_due = Some((Instant::now() + c.after, c.url));
        }
        for HttpNeed { purpose, url, headers } in step.fetch {
            self.online_get(Purpose::Session(session, purpose), url, headers);
        }
        self.online_connect_due(Instant::now());
        for update in step.updates {
            if self.online.follow.as_ref().is_none_or(|f| f.session != session) {
                break;
            }
            self.online_update(session, update);
        }
    }

    fn online_connect_due(&mut self, now: Instant) {
        let Some(f) = self.online.follow.as_mut() else { return };
        if f.connect_due.as_ref().is_some_and(|(due, _)| now >= *due) {
            let (_, url) = f.connect_due.take().expect("due");
            if let Some(old) = f.conn.take() {
                self.online.jobs.push(OnlineJob::Close { conn: old });
            }
            self.online.next_id += 1;
            let conn = self.online.next_id;
            let f = self.online.follow.as_mut().expect("follow");
            f.conn = Some(conn);
            // huanle's WebSocketClient goes through NetworkProxy; the 弈客 socket.io client
            // takes default options, so it always connects directly.
            let proxied = matches!(f.transport, Transport::Huanle(_));
            self.online.jobs.push(OnlineJob::Connect { conn, url, proxied });
        }
    }

    /// A reply to a follow's own read: the type-6 poll (`reqNewYikeRoom` OD 1465-1618), type 2
    /// (`refresh`) or a plain link (`get`).
    fn online_follow_reply(&mut self, session: u64, reply: HttpReply) {
        let Some(f) = self.online.follow.as_mut().filter(|f| f.session == session) else { return };
        let dialog = f.origin == Origin::Dialog;
        let (status, body) = match reply {
            HttpReply::Ok { status, body } => (status, body),
            HttpReply::Failed { proxy, message } => {
                // A failed read keeps polling (`scheduleWithFixedDelay`); a proxy that cannot be
                // used ends it (`invalidProxyConfig`).
                if let Transport::Poll { in_flight, due, interval, .. } = &mut f.transport {
                    *in_flight = false;
                    *due = (!proxy).then(|| Instant::now() + *interval);
                }
                if proxy {
                    self.online_proxy_warning(&message);
                }
                self.online_failed(session, &message, dialog);
                return;
            }
        };
        match &mut f.transport {
            Transport::Poll { in_flight, due, interval, .. } => {
                *in_flight = false;
                *due = Some(Instant::now() + *interval);
                if status >= 400 {
                    let message = format!("Yike request failed: HTTP {status} {body}");
                    self.online_failed(session, &message, dialog);
                    return;
                }
                let payload = match yike::parse_detail(&body) {
                    Ok(p) => p,
                    Err(message) => {
                        self.online_failed(session, &message, dialog);
                        return;
                    }
                };
                let f = self.online.follow.as_mut().expect("follow");
                if payload.finished {
                    if let Transport::Poll { due, .. } = &mut f.transport {
                        *due = None;
                    }
                }
                if payload.sgf.trim().is_empty() {
                    self.online_status(OnlineText::new("YikeLiveDialog.syncNoSgf"));
                    self.online.reveal += 1;
                    return;
                }
                let mainline = yike::mainline_only(&payload.sgf);
                let finished = payload.finished;
                if mainline != f.last_mainline {
                    f.last_mainline = mainline;
                    self.online_payload(session, payload, false);
                }
                let key = if finished { "YikeLiveDialog.syncedFinished" } else { "YikeLiveDialog.syncedWatching" };
                self.online_status(OnlineText::new(key));
            }
            Transport::Once { plain } => {
                let plain = *plain;
                if let Some(live) = yike::live_object(&body) {
                    let payload = yike::payload_of(&live);
                    if payload.sgf.trim().is_empty() {
                        self.online_status(OnlineText::new("YikeLiveDialog.syncNoSgf"));
                        self.online.reveal += 1;
                        return;
                    }
                    self.online_payload(session, payload, false);
                } else if plain {
                    // get() → parseSgf(sgf, "", …): loadSgfString, a record of its own.
                    let no_capture = self.config.edit.no_capture;
                    let read = formats::parse(RecordFormat::Sgf, &body, no_capture).map(|r| (RecordFormat::Sgf, r));
                    self.online.follow = None;
                    let ticket = self.begin_open(OpenOrigin::Online { follow: false });
                    self.complete_open(ticket, read);
                } else {
                    self.online_failed(session, "Yike live detail response does not contain result.", dialog);
                }
            }
            _ => {}
        }
    }

    /// 弈客同步失败: … and, from 打开在线链接, 无效的链接 again (`error(true)`).
    fn online_failed(&mut self, session: u64, message: &str, dialog: bool) {
        if self.online.follow.as_ref().is_none_or(|f| f.session != session) {
            return;
        }
        self.online_status(OnlineText::with("YikeLiveDialog.syncFailed", [message.to_string()]));
        if dialog {
            self.online.dialog_error += 1;
        }
    }

    /// A REST / page payload: the SGF's main line as the source's game.
    fn online_payload(&mut self, session: u64, payload: yike::LivePayload, set_result: bool) {
        let no_capture = self.config.edit.no_capture;
        let dialog = self.online.follow.as_ref().is_some_and(|f| f.origin == Origin::Dialog);
        let record = match formats::parse(RecordFormat::Sgf, &yike::mainline_only(&payload.sgf), no_capture) {
            Ok(r) => r,
            Err(e) => {
                self.online_failed(session, &e.to_string(), dialog);
                return;
            }
        };
        let names = match (payload.black, payload.white) {
            (None, None) => None,
            (b, w) => Some((b.unwrap_or_else(|| record.info.black.clone()), w.unwrap_or_else(|| record.info.white.clone()))),
        };
        self.online_update(
            session,
            SourceUpdate::Game { record: Box::new(record), names, komi: payload.komi, handicap: payload.handicap },
        );
        if payload.finished {
            self.online_update(session, SourceUpdate::Finished { result: payload.result, set_result });
        }
    }

    // ---------------------------------------------------------------- applying updates

    fn online_update(&mut self, session: u64, update: SourceUpdate) {
        let Some(f) = self.online.follow.as_mut().filter(|f| f.session == session) else { return };
        if f.adopting.is_some() && !matches!(update, SourceUpdate::Status(_)) {
            // The first game is still being adopted (the open waits for a borrowed engine):
            // what the source reports meanwhile follows it.
            f.queued.push(update);
            return;
        }
        // The user went on to another record or game: Java's board clear stops the follow.
        if f.doc.is_some_and(|d| d != self.doc.id) {
            self.online_stop();
            return;
        }
        // Before the room's first game there is no record of the room to write into (Java's
        // `history` is still null and the write fails).
        if f.doc.is_none()
            && matches!(update, SourceUpdate::Comment { .. } | SourceUpdate::CommentUpdate { .. } | SourceUpdate::Branch { .. })
        {
            return;
        }
        // While 野狐 AI 直连 owns the board, Java's Board refuses the source's writes
        // (`setHistory` → `FoxLiveSession.rejectOrdinaryMutation`, Board.java:5270,
        // FoxLiveSession.java:406-411); the follow keeps reading.
        if self.fox_owns_board() && !matches!(update, SourceUpdate::Status(_) | SourceUpdate::Stopped(_)) {
            return;
        }
        match update {
            SourceUpdate::Game { record, names, komi, handicap } => self.online_game(*record, names, komi, handicap),
            SourceUpdate::Finished { result, set_result } => self.online_finished(&result, set_result),
            SourceUpdate::NoGame { finished } => self.online_no_game(finished),
            SourceUpdate::Comment { move_number, text } => self.online_comment(move_number, None, &text),
            SourceUpdate::CommentUpdate { move_number, old, text } => self.online_comment(move_number, Some(&old), &text),
            SourceUpdate::Branch { move_number, actions } => self.online_branch(move_number, actions),
            SourceUpdate::Status(text) => {
                self.online_status(text.clone());
                if self.online.follow.as_ref().is_some_and(|f| !f.source.is_yike()) {
                    self.online.browser_status = Some(text);
                }
            }
            SourceUpdate::Stopped(text) => {
                self.online_stop();
                self.online.browser_status = Some(text);
            }
        }
    }

    fn online_game(&mut self, mut record: GameRecord, names: Option<(String, String)>, komi: Option<f64>, handicap: Option<u32>) {
        if let Some((black, white)) = &names {
            record.info.black.clone_from(black);
            record.info.white.clone_from(white);
        }
        if let Some(h) = handicap {
            record.info.handicap = h;
        }
        let f = self.online.follow.as_ref().expect("follow");
        let adopt = f.doc.is_none()
            || self.doc.record.size != record.size
            || self.doc.record.node(ROOT).setup != record.node(ROOT).setup;
        let line = source_line(&record);
        if adopt || line.is_none() {
            self.online_adopt(record, komi);
            return;
        }
        let line = line.expect("checked");
        let expected = line.len();
        // resolveLiveKomi / applyLiveKomi on each update of the game (with 自动读取贴目): the
        // source's komi unless the user set one.
        let source_komi = komi
            .or(record.komi_declared.then_some(record.info.komi))
            .filter(|_| self.config.record.read_komi && !self.komi_changed);
        // syncLiveBoardStat: browsing follows a new end on the first sync, from the previous
        // live end on the main line, or with 总是跳转到最新一手.
        let f = self.online.follow.as_ref().expect("follow");
        let previous_end = f.live_end;
        let at_end = previous_end == Some(self.cursor) && self.doc.record.is_main_trunk(self.cursor);
        let rec = &self.doc.record;
        let retractable: Vec<NodeId> = f
            .made
            .iter()
            .filter(|(&n, comment)| rec.contains(n) && rec.node(n).comment == **comment && rec.node(n).marks.is_empty())
            .map(|(&n, _)| n)
            .collect();
        let before = rec.node_count() as NodeId;
        let cursor_path = rec.path_to(self.cursor);
        let rules = self.play_rules();
        let Ok(applied) = self.apply_edit(DocumentEdit::SyncMainline { line, retractable, rules }) else { return };
        let end = applied.node.unwrap_or(ROOT);
        let rec = &self.doc.record;
        if (rec.node(end).move_number as usize) < expected && rec.path_to(end).len() <= expected {
            // A move the document refuses (its start was edited): take the game again.
            self.online_adopt(record, komi);
            return;
        }
        let f = self.online.follow.as_mut().expect("follow");
        f.made.retain(|&n, _| rec.contains(n));
        for n in rec.path_to(end) {
            if n >= before {
                f.made.insert(n, rec.node(n).comment.clone());
            }
        }
        f.live_end = Some(end);
        if !rec.contains(self.cursor) {
            self.cursor = cursor_path.iter().rev().copied().find(|&n| rec.contains(n)).unwrap_or(ROOT);
        }
        if let Some((black, white)) = names {
            if rec.info.black != black || rec.info.white != white {
                let _ = self.apply_edit(DocumentEdit::SetPlayers { black, white });
            }
        }
        if let Some(h) = handicap {
            let _ = self.apply_edit(DocumentEdit::SetHandicap { handicap: h });
        }
        if Some(end) != previous_end && (at_end || self.config.live.always_goto_last) {
            self.move_to(end);
        }
        let komi_applied = source_komi.is_some_and(|k| self.apply_komi(k));
        if applied.changed || komi_applied {
            self.sync_engine();
        }
        if applied.changed {
            self.online_schedule_curve();
        }
    }

    /// The first game (or a game whose size or start changed): read into a new document through
    /// the open path, like Java's `setHistory` of the parsed live game.
    fn online_adopt(&mut self, mut record: GameRecord, komi: Option<f64>) {
        if let Some(k) = komi {
            record.info.komi = k;
            record.komi_declared = true;
        }
        // resolveLiveKomi: a komi the user set by hand stays, and stays theirs.
        let keep_komi = self.komi_changed;
        if keep_komi {
            record.info.komi = self.komi;
            record.komi_declared = false;
        }
        let ticket = self.begin_open(OpenOrigin::Online { follow: true });
        if let Some(f) = self.online.follow.as_mut() {
            f.adopting = Some(ticket);
            f.keep_komi = keep_komi;
        }
        self.complete_open(ticket, Ok((RecordFormat::Sgf, record)));
    }

    /// A record was adopted by the open path; the follow's game when it is that ticket, else
    /// the user opened something else and the follow ends.
    pub(super) fn online_adopted(&mut self, ticket: u64) {
        let Some(f) = self.online.follow.as_mut() else { return };
        if f.adopting != Some(ticket) {
            self.online_stop();
            return;
        }
        f.adopting = None;
        f.doc = Some(self.doc.id);
        if f.keep_komi {
            self.komi_changed = true;
        }
        let rec = &self.doc.record;
        let end = rec.line_end(ROOT);
        f.made = rec.path_to(end).into_iter().filter(|&n| n != ROOT).map(|n| (n, rec.node(n).comment.clone())).collect();
        f.live_end = Some(end);
        let session = f.session;
        let queued = std::mem::take(&mut f.queued);
        self.online_schedule_curve();
        for update in queued {
            self.online_update(session, update);
        }
    }

    pub(super) fn online_open_failed(&mut self, ticket: u64) {
        let Some(f) = self.online.follow.as_ref().filter(|f| f.adopting == Some(ticket)) else { return };
        let session = f.session;
        let dialog = f.origin == Origin::Dialog;
        if let Some(f) = self.online.follow.as_mut() {
            f.adopting = None;
        }
        self.online_failed(session, "", dialog);
    }

    /// The game ended (OD 1227-1245, 4399-4416): the result goes to the top of the live end's
    /// comment, once.
    fn online_finished(&mut self, result: &str, set_result: bool) {
        let Some(f) = self.online.follow.as_mut() else { return };
        if f.finished {
            return;
        }
        f.finished = true;
        let Some(end) = f.live_end.filter(|&n| self.doc.record.contains(n)) else { return };
        if !result.trim().is_empty() {
            let comment = format!("{result}\n{}", self.doc.record.node(end).comment);
            self.online_set_comment(end, comment);
            if set_result {
                let _ = self.apply_edit(DocumentEdit::SetResult { result: result.to_string() });
            }
        }
    }

    /// A source comment write. A node the follow made stays one it may take back only while the
    /// user has not changed it (its comment is still the follow's).
    fn online_set_comment(&mut self, node: NodeId, comment: String) {
        let current = &self.doc.record.node(node).comment;
        let untouched = self.online.follow.as_ref().is_some_and(|f| f.made.get(&node) == Some(current));
        if self.apply_edit(DocumentEdit::SetComment { node, comment: comment.clone() }).is_ok() && untouched {
            if let Some(f) = self.online.follow.as_mut() {
                f.made.insert(node, comment);
            }
        }
    }

    /// The main line node with `move_number` (`goToMoveNumber` on the synced history).
    fn online_main_node(&self, move_number: u32) -> Option<NodeId> {
        let rec = &self.doc.record;
        let mut at = ROOT;
        while rec.node(at).move_number < move_number {
            at = rec.main_child(at)?;
        }
        (rec.node(at).move_number == move_number).then_some(at)
    }

    /// A source comment on its own line of the node's comment; `replaces` names the line an
    /// updated comment had, which is replaced where it still stands.
    fn online_comment(&mut self, move_number: u32, replaces: Option<&str>, text: &str) {
        let Some(node) = self.online_main_node(move_number) else { return };
        let current = &self.doc.record.node(node).comment;
        let mut lines: Vec<&str> = current.lines().collect();
        match replaces.and_then(|old| lines.iter().position(|l| *l == old)) {
            Some(i) => lines[i] = text,
            None => lines.push(text),
        }
        let comment = lines.join("\n");
        if comment == *current {
            return;
        }
        self.online_set_comment(node, comment);
    }

    /// A room without a readable game (`initData` with no history): the board stays; a finished
    /// room is read once from the old detail API (`refresh`, OD 3341-3352).
    fn online_no_game(&mut self, finished: bool) {
        let Some(f) = self.online.follow.as_mut() else { return };
        let detail = match &f.source {
            OnlineSource::YikeLiveRoom { detail, .. } | OnlineSource::YikeGameRoom { detail, .. } => detail.clone(),
            _ => return,
        };
        if !finished {
            return;
        }
        f.transport = Transport::Once { plain: false };
        let session = f.session;
        self.online_get(Purpose::Follow(session), detail, Vec::new());
    }

    /// A commentator variation (`addBranch` → `SGFParser.parseBranch`): added as a new branch at
    /// the main line node `move_number`.
    fn online_branch(&mut self, move_number: u32, actions: Vec<SourceAction>) {
        let Some(at) = self.online_main_node(move_number) else { return };
        let rules = self.play_rules();
        let _ = self.apply_edit(DocumentEdit::AddSourceBranch { at, line: actions, rules });
    }

    // ---------------------------------------------------------------- 曲线补全

    /// `scheduleYikeLiveCurveCompletion`: with 打开棋谱后自动快速分析 on, the missing win-rate
    /// curve is filled 1.2 s after the last update.
    fn online_schedule_curve(&mut self) {
        let yike = self.online.follow.as_ref().is_some_and(|f| f.source.is_yike());
        if !yike || !self.config.record.auto_quick_analyze_on_load {
            return;
        }
        self.online.curve = Some(Curve { due: Instant::now() + CURVE_DELAY, started: false, doc: self.doc.id });
    }

    fn online_curve_tick(&mut self, now: Instant) -> bool {
        let Some(curve) = self.online.curve.as_ref() else { return false };
        let (due, started, doc) = (curve.due, curve.started, curve.doc);
        if now < due {
            return false;
        }
        if doc != self.doc.id {
            self.online.curve = None;
            return false;
        }
        let busy = self.quick_busy();
        if busy {
            self.online.curve.as_mut().expect("curve").due = now + CURVE_RETRY;
        }
        if started {
            if busy {
                return false;
            }
            self.online.curve = None;
            self.online_status(OnlineText::new("YikeLiveDialog.curveUpdated"));
            return true;
        }
        if busy {
            self.online_status(OnlineText::new("YikeLiveDialog.curveWaiting"));
            return true;
        }
        let missing = self.quick_missing().len();
        if missing == 0 {
            self.online.curve = None;
            return false;
        }
        if !self.quick_allowed() {
            self.online.curve = None;
            return false;
        }
        self.quick_start();
        if !self.quick_busy() {
            self.online.curve = None;
            self.online_status(OnlineText::new("YikeLiveDialog.curveFailed"));
            return true;
        }
        let curve = self.online.curve.as_mut().expect("curve");
        curve.started = true;
        curve.due = now + CURVE_RETRY;
        self.online_status(OnlineText::with("YikeLiveDialog.curveCompleting", [missing.to_string()]));
        true
    }

    // ---------------------------------------------------------------- timers

    pub(super) fn online_deadline(&self) -> Option<Instant> {
        let f = self.online.follow.as_ref();
        let poll = f.and_then(|f| match &f.transport {
            Transport::Poll { due, in_flight: false, .. } => *due,
            Transport::Rtgame(s) => s.next_due(),
            Transport::Huanle(s) => s.next_due(),
            _ => None,
        });
        let connect = f.and_then(|f| f.connect_due.as_ref().map(|(d, _)| *d));
        let curve = self.online.curve.as_ref().map(|c| c.due);
        [poll, connect, curve].into_iter().flatten().min()
    }

    pub(super) fn online_tick(&mut self, now: Instant) -> bool {
        let mut changed = self.online_curve_tick(now);
        self.online_connect_due(now);
        let Some(f) = self.online.follow.as_mut() else { return changed };
        let session = f.session;
        match &mut f.transport {
            Transport::Poll { url, due, in_flight, .. } => {
                if !*in_flight && due.is_some_and(|d| now >= d) {
                    *in_flight = true;
                    *due = None;
                    let url = url.clone();
                    self.online_get(Purpose::Follow(session), url, yike::headers(now_ms(), nonce()));
                }
            }
            Transport::Rtgame(s) if s.next_due().is_some_and(|d| now >= d) => {
                let step = s.tick(now);
                self.online_step(session, step);
                changed = true;
            }
            Transport::Huanle(s) if s.next_due().is_some_and(|d| now >= d) => {
                let step = s.tick(now);
                self.online_step(session, step);
                changed = true;
            }
            _ => {}
        }
        changed || !self.online.jobs.is_empty()
    }
}


//! What an 打开在线链接 link or a 弈客 page address refers to (`YikeUrlParser.parse`, then the rest
//! of `OnlineDialog.checkUrl`, fixed Java `cb53acc`). The first match wins; anything left is a
//! plain link that is fetched once and read as SGF (type 99).

use std::sync::LazyLock;

use fancy_regex::Regex;

/// Yike detail endpoint of new live rooms (`YikeApiClient.DETAIL_URL_PREFIX`).
pub const YIKE_DETAIL_PREFIX: &str = "https://api-new.yikeweiqi.com/v1/golives/";

/// The followed source of a link (`YikeUrlInfo` types and `checkUrl`'s 3, 4 and 99).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OnlineSource {
    /// Type 6, `live/new-room/<id>…`: the signed detail endpoint, polled.
    YikeNewLive { id: String, room: u64 },
    /// Type 1, `live/room/<id>/<n>/<room>` (and other `live/<word>/` paths): Socket.IO room.
    /// `detail` is the old `golive/dtl` read (`ajaxUrl`) a finished room without SGF falls back to.
    YikeLiveRoom { id: String, room: u64, detail: String },
    /// Type 2, `live/room/<id>` or a non-numeric room: one read of `golive/dtl`.
    YikeLiveBoard { id: String, url: String },
    /// Type 5, `game/<word>/<n>/<room>` or `room=<n>&hall`: Socket.IO room; `detail` as type 1.
    YikeGameRoom { room: u64, detail: String },
    /// Type 7, `unite/<room>` (对局室): read through the 弈客 web page's own requests.
    YikeUnite { room: u64, info_url: String },
    /// Type 3, `gameid` and `createtime` in the query (野狐分享 links): the huanle broadcast.
    HuanleGame { query: Vec<(String, String)> },
    /// Type 4, `gametag` and `uin` in the query (腾讯欢乐 links): a JSONP request, then the
    /// huanle broadcast.
    HuanleTag { query: Vec<(String, String)>, request_url: String },
    /// Type 99: any other link, fetched once.
    Plain { url: String },
}

impl OnlineSource {
    /// `OnlineDialog.isYikeSyncType`: types 1, 2, 5, 6 and 7.
    pub fn is_yike(&self) -> bool {
        matches!(
            self,
            OnlineSource::YikeNewLive { .. }
                | OnlineSource::YikeLiveRoom { .. }
                | OnlineSource::YikeLiveBoard { .. }
                | OnlineSource::YikeGameRoom { .. }
                | OnlineSource::YikeUnite { .. }
        )
    }

    /// The Java type number, used for status prefixes and in diagnostics.
    pub fn java_type(&self) -> u8 {
        match self {
            OnlineSource::YikeLiveRoom { .. } => 1,
            OnlineSource::YikeLiveBoard { .. } => 2,
            OnlineSource::HuanleGame { .. } => 3,
            OnlineSource::HuanleTag { .. } => 4,
            OnlineSource::YikeGameRoom { .. } => 5,
            OnlineSource::YikeNewLive { .. } => 6,
            OnlineSource::YikeUnite { .. } => 7,
            OnlineSource::Plain { .. } => 99,
        }
    }
}

fn re(pattern: &str) -> Regex {
    Regex::new(&format!("^(?:{pattern})$")).expect("valid pattern")
}

// YikeUrlParser.java:9-25. `matches()` is a whole-string match.
static NEW_LIVE_FULL: LazyLock<Regex> = LazyLock::new(|| {
    re(r"https*://(?s).*?([^\./]+\.[^\./]+)/(?s).*?(live/new-room/)([^/]+)/[0-9]+/([^/\s]+)[^\n]*")
});
static NEW_LIVE_SHORT: LazyLock<Regex> =
    LazyLock::new(|| re(r"https*://(?s).*?([^\./]+\.[^\./]+)/(?s).*?(live/new-room/)([^/\s]+)"));
static OLD_LIVE_FULL: LazyLock<Regex> = LazyLock::new(|| {
    re(r"https*://(?s).*?([^\./]+\.[^\./]+)/(?s).*?(live/room/)([^/]+)/[0-9]+/([^/\s]+)[^\n]*")
});
static OLD_LIVE_SHORT: LazyLock<Regex> =
    LazyLock::new(|| re(r"https*://(?s).*?([^\./]+\.[^\./]+)/(?s).*?(live/room/)([^/\s]+)"));
static GAME_ROOM: LazyLock<Regex> =
    LazyLock::new(|| re(r"https*://(?s).*?([^\./]+\.[^\./]+)/(?s).*?(game/[a-zA-Z]+/)[0-9]+/([^/\s]+)"));
static HALL_ROOM: LazyLock<Regex> =
    LazyLock::new(|| re(r"https*://(?s).*?([^\./]+\.[^\./]+)/(?s).*?(room=)([0-9]+)(&hall)(?s).*?"));
static UNITE_ROOM: LazyLock<Regex> =
    LazyLock::new(|| re(r"https*://(?s).*?([^\./]+\.[^\./]+)/(?s).*?(unite/)([0-9]+)(?s).*?"));
// OnlineDialog.java:756-807, after the parser.
static LEGACY_LIVE_FULL: LazyLock<Regex> = LazyLock::new(|| {
    re(r"https*://(?s).*?([^\./]+\.[^\./]+)/(?s).*?(live/[a-zA-Z]+/)([^/]+)/[0-9]+/([^/]+)[^\n]*")
});
static LEGACY_LIVE_SHORT: LazyLock<Regex> =
    LazyLock::new(|| re(r"https*://(?s).*?([^\./]+\.[^\./]+)/(?s).*?(live/[a-zA-Z]+/)([^/]+)"));
static LEGACY_GAME: LazyLock<Regex> =
    LazyLock::new(|| re(r"https*://(?s).*?([^\./]+\.[^\./]+)/(?s).*?(game/[a-zA-Z]+/)[0-9]+/([^/]+)"));

fn caps<'t>(regex: &Regex, text: &'t str) -> Option<Vec<&'t str>> {
    let c = regex.captures(text).ok()??;
    Some((0..c.len()).map(|i| c.get(i).map_or("", |m| m.as_str())).collect())
}

fn number(text: &str) -> Option<i64> {
    text.parse().ok()
}

/// The new live detail endpoint of `id` (`YikeApiClient.detailUrl`, URL-encoded).
pub fn yike_detail_url(id: &str) -> String {
    format!("{YIKE_DETAIL_PREFIX}{}", form_encode(id))
}

/// `URLEncoder.encode(value, UTF_8)`.
pub fn form_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'-' | b'*' | b'_' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn yike(url: &str) -> Option<OnlineSource> {
    let url = url.strip_suffix("/0/0").unwrap_or(url);
    if let Some(c) = caps(&NEW_LIVE_FULL, url) {
        let id = c[3].to_string();
        let room = number(c[4]).or(number(&id)).unwrap_or(0).max(0) as u64;
        return Some(OnlineSource::YikeNewLive { id, room });
    }
    if let Some(c) = caps(&NEW_LIVE_SHORT, url) {
        let id = c[3].to_string();
        let room = number(&id).unwrap_or(0).max(0) as u64;
        return Some(OnlineSource::YikeNewLive { id, room });
    }
    if let Some(c) = caps(&OLD_LIVE_FULL, url) {
        let id = c[3];
        let (room, board) = match number(c[4]) {
            Some(r) if r >= 0 => (r, false),
            _ => (number(id).unwrap_or(0), true),
        };
        if !id.trim().is_empty() && room > 0 {
            let detail = format!("https://api.{}/golive/dtl?id={id}&flag=1", c[1]);
            return Some(if board {
                OnlineSource::YikeLiveBoard { id: id.into(), url: detail }
            } else {
                OnlineSource::YikeLiveRoom { id: id.into(), room: room as u64, detail }
            });
        }
    }
    if let Some(c) = caps(&OLD_LIVE_SHORT, url) {
        if !c[3].trim().is_empty() {
            return Some(OnlineSource::YikeLiveBoard {
                id: c[3].into(),
                url: format!("https://api.{}/golive/dtl?id={}", c[1], c[3]),
            });
        }
    }
    for regex in [&*GAME_ROOM, &*HALL_ROOM] {
        if let Some(c) = caps(regex, url) {
            if let Some(room) = number(c[3]).filter(|&r| r > 0) {
                let detail = format!("https://api.{}/golive/dtl?id={room}", c[1]);
                return Some(OnlineSource::YikeGameRoom { room: room as u64, detail });
            }
        }
    }
    if let Some(c) = caps(&UNITE_ROOM, url) {
        if let Some(room) = number(c[3]).filter(|&r| r > 0) {
            return Some(OnlineSource::YikeUnite {
                room: room as u64,
                info_url: format!(
                    "https://game-server.yikeweiqi.com/game/info?id={room}&sgf_option=true&players_option=true&setting_option=true&clock_option=true&is_void=true"
                ),
            });
        }
    }
    None
}

/// Classifies an 打开在线链接 link (`checkUrl`); only a blank link has no source.
pub fn classify(url: &str) -> Option<OnlineSource> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    if let Some(source) = yike(url) {
        return Some(source);
    }
    let trimmed = url.strip_suffix("/0/0").unwrap_or(url);
    if let Some(c) = caps(&LEGACY_LIVE_FULL, trimmed) {
        let id = c[3];
        let (room, board) = match number(c[4]) {
            Some(r) => (r, false),
            None => (number(id).unwrap_or(0), true),
        };
        if !id.trim().is_empty() && room > 0 {
            let detail = format!("https://api.{}/golive/dtl?id={id}&flag=1", c[1]);
            return Some(if board {
                OnlineSource::YikeLiveBoard { id: id.into(), url: detail }
            } else {
                OnlineSource::YikeLiveRoom { id: id.into(), room: room as u64, detail }
            });
        }
    }
    if let Some(c) = caps(&LEGACY_LIVE_SHORT, trimmed) {
        if !c[3].trim().is_empty() {
            return Some(OnlineSource::YikeLiveBoard {
                id: c[3].into(),
                url: format!("https://api.{}/golive/dtl?id={}", c[1], c[3]),
            });
        }
    }
    if let Some(c) = caps(&LEGACY_GAME, trimmed) {
        if let Some(room) = number(c[3]).filter(|&r| r > 0) {
            let detail = format!("https://api.{}/golive/dtl?id={room}", c[1]);
            return Some(OnlineSource::YikeGameRoom { room: room as u64, detail });
        }
    }
    if let Some((query, raw)) = split_query(trimmed) {
        let has = |k: &str| query.iter().any(|(key, _)| key == k);
        if has("gameid") && has("createtime") {
            return Some(OnlineSource::HuanleGame { query });
        }
        if has("gametag") && has("uin") {
            let request_url =
                format!("http://wshall.huanle.qq.com/wxnseed/Broadcast/RequestBroadcast?callback=jQuery1&{raw}");
            return Some(OnlineSource::HuanleTag { query, request_url });
        }
    }
    Some(OnlineSource::Plain { url: trimmed.to_string() })
}

/// The 弈客 page kind of an address (`BrowserFrame.yikePageKind`, used for the room key that
/// keeps one room from being followed twice).
pub fn yike_room_key(url: &str) -> Option<String> {
    match yike(url.trim())? {
        OnlineSource::YikeNewLive { id, .. }
        | OnlineSource::YikeLiveRoom { id, .. }
        | OnlineSource::YikeLiveBoard { id, .. } => Some(format!("live-room:{id}")),
        OnlineSource::YikeGameRoom { room, .. } | OnlineSource::YikeUnite { room, .. } => Some(format!("unite-board:{room}")),
        _ => None,
    }
}

/// `OnlineDialog.splitQuery` over `URI.getQuery()`: the percent-decoded query before any
/// fragment, split on `&` and the first `=` (a key without `=` has an empty value). Also returns
/// the raw query (`getRawQuery`).
fn split_query(url: &str) -> Option<(Vec<(String, String)>, String)> {
    let before_fragment = url.split('#').next().unwrap_or("");
    let (_, raw) = before_fragment.split_once('?')?;
    let decoded = percent_decode(raw);
    let pairs = decoded
        .split('&')
        .map(|it| match it.find('=') {
            Some(i) if i > 0 => (it[..i].to_string(), it[i + 1..].to_string()),
            _ => (it.to_string(), String::new()),
        })
        .collect();
    Some((pairs, raw.to_string()))
}

/// `%XX` escapes as UTF-8 (`URI` decoding; `+` stays as it is).
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = |b: u8| (b as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

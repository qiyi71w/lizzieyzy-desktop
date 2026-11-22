//! 弈客 REST requests and responses (`YikeApiClient`, fixed Java `cb53acc`): the signed headers,
//! the live list (弈客直播中心) and the new live room detail.

use md5::Md5;
use serde::Serialize;
use serde_json::Value;
use sha1::{Digest, Sha1};
use ts_rs::TS;

use super::url::form_encode;

/// `YikeApiClient.YIKE_LIVE_URL` / `YIKE_GAME_URL`: 打开弈客网页版 and 弈客大厅.
pub const YIKE_LIVE_PAGE: &str = "https://home.yikeweiqi.com/#/live";
pub const YIKE_GAME_PAGE: &str = "https://home.yikeweiqi.com/#/game";
const LIST_URL: &str = "https://api.yikeweiqi.com/v2/golive/list";
// The web client's public request signing (YikeApiClient.java:30-31, 127).
const APP_KEY: &str = "3396jtzhK57XhJom";
const APP_SECRET: &str = "hfdSXRKm0DQyLmNXmNCNkZpjy2o5q1Hk";
const ACCESS_SALT: &str = "@1%e$5*f@3";
/// `HTTP_TIMEOUT_MS`: connect and read.
pub const HTTP_TIMEOUT_MS: u64 = 10_000;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn md5_hex(text: &str) -> String {
    hex(&Md5::digest(text.as_bytes()))
}

/// `YikeApiClient.buildHeaders(currentTimeMillis, nonce)`, in Java's order.
pub fn headers(now_ms: u64, nonce: u32) -> Vec<(String, String)> {
    let time = now_ms.to_string();
    let nonce = nonce.to_string();
    let check = hex(&Sha1::digest(format!("{APP_SECRET}{nonce}{time}").as_bytes()));
    let access = md5_hex(&format!("{ACCESS_SALT}{}web", md5_hex(&time)));
    [
        ("AppKey", APP_KEY.to_string()),
        ("CurTime", time.clone()),
        ("CheckSum", check),
        ("Nonce", nonce),
        ("usertoken", "-1".into()),
        ("version", "96813".into()),
        ("Platform", "web".into()),
        ("Content-Type", "application/json".into()),
        ("timestamp", time),
        ("uuid", "web".into()),
        ("accept-language", "zh-cn".into()),
        ("accesstoken", access),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

/// 弈客直播中心's 分类 (`YikeLiveDialog.Category`): the `official` code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, serde::Deserialize, TS)]
pub enum LiveCategory {
    /// 推荐, `official=1` (the first entry).
    #[default]
    Recommend,
    /// 地方, `official=0`.
    Local,
    /// 个人, `official=4`.
    Personal,
}

impl LiveCategory {
    fn code(self) -> &'static str {
        match self {
            LiveCategory::Recommend => "1",
            LiveCategory::Local => "0",
            LiveCategory::Personal => "4",
        }
    }
}

/// `fetchLiveList(official, page, since)`.
pub fn list_url(category: LiveCategory, page: u32, since: i64) -> String {
    format!(
        "{LIST_URL}?p={}&since={}&official={}&version=2",
        page.max(1),
        since.max(0),
        form_encode(category.code())
    )
}

/// 胜率 of a game under realtime analysis (`YikeLiveGame.winrateText`'s numbers): the UI words them
/// in the interface language.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, TS)]
pub struct LiveWinrate {
    /// `BlackWinRate`, in percent.
    pub black: f64,
    /// `100 - BlackWinRate`, kept within 0..=100.
    pub white: f64,
    /// `Delta` in points; `None` within 0.01 of 0, which the column leaves out.
    pub score: Option<f64>,
}

/// One row of the live list (`YikeLiveGame`) with what the table shows.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct LiveGameView {
    /// 状态: `*` for a pinned game, then 直播预告 / 正在直播 / the result or 已结束 / 未知.
    pub status: String,
    /// 时间.
    pub time: String,
    /// 棋局.
    pub game: String,
    /// 黑方 / 白方: the name and ` [county]`.
    pub black: String,
    pub white: String,
    /// 手数.
    pub hands: i64,
    /// 胜率: `None` without realtime analysis or a black win rate.
    pub rate: Option<LiveWinrate>,
    /// 关注.
    pub attention: String,
    /// The room the 同步 button follows (`toRoomUrl`); `None` for a preview without moves
    /// (`isPreviewWithoutMoves`), which 同步 refuses with 这局暂时还没有可用棋谱。.
    pub room_url: Option<String>,
}

fn text(item: &Value, key: &str) -> String {
    // org.json optString: a missing key or null is "", other values are their string form.
    match item.get(key) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
    }
}

fn int(item: &Value, key: &str, default: i64) -> i64 {
    match item.get(key) {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(default),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(default),
        Some(Value::Bool(b)) => i64::from(*b),
        _ => default,
    }
}

fn float(item: &Value, key: &str, default: f64) -> f64 {
    match item.get(key) {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(default),
        Some(Value::String(s)) => s.trim().parse().unwrap_or(default),
        _ => default,
    }
}

fn blank(s: &str) -> bool {
    s.trim().is_empty()
}

fn live_game(item: &Value) -> LiveGameView {
    let id = int(item, "Id", 0);
    let version = int(item, "Version", 1);
    let status = int(item, "Status", 0);
    let result = text(item, "GameResult");
    let hands = int(item, "HandsCount", 0);
    let status_text = match status {
        1 => "直播预告".to_string(),
        2 => "正在直播".to_string(),
        3 if blank(&result) => "已结束".to_string(),
        3 => result,
        _ => "未知".to_string(),
    };
    let finish = text(item, "FinishOrder");
    let date = if blank(&finish) { text(item, "GameDate") } else { finish };
    let broadcast = text(item, "BroadcastTime");
    let time = if blank(&date) {
        broadcast
    } else if blank(&broadcast) {
        date
    } else {
        format!("{date} {broadcast}")
    };
    let player = |name: &str, county: &str| {
        let (name, county) = (text(item, name), text(item, county));
        if blank(&county) { name } else { format!("{name} [{county}]") }
    };
    let black_rate = float(item, "BlackWinRate", -1.0);
    let delta = float(item, "Delta", 0.0);
    let realtime = int(item, "RealtimeAnalysisFlag", 0) == 1 && black_rate >= 0.0;
    let rate = realtime.then(|| LiveWinrate {
        black: black_rate,
        white: (100.0 - black_rate).clamp(0.0, 100.0),
        score: (delta.abs() > 0.01).then_some(delta),
    });
    let pinned = if int(item, "TopFlag", 0) == 1 { "*" } else { "" };
    let room_path = if version == 2 { "new-room" } else { "room" };
    let room_url = (!(status == 1 && hands <= 0)).then(|| {
        format!(
            "https://home.yikeweiqi.com/#/live/{room_path}/{id}/{}/{}",
            int(item, "hall", 0),
            int(item, "room", 0)
        )
    });
    LiveGameView {
        status: format!("{pinned}{status_text}"),
        time,
        game: text(item, "GameName"),
        black: player("BlackName", "BlackCounty"),
        white: player("WhiteName", "WhiteCounty"),
        hands,
        rate,
        attention: int(item, "PersonTimes", 0).to_string(),
        room_url,
    }
}

/// `parseLiveList`: `Status` 1200 or the `Message` as the failure; the page's games and the
/// `since` for 更多.
pub fn parse_list(body: &str) -> Result<(i64, Vec<LiveGameView>), String> {
    let root: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    if int(&root, "Status", 0) != 1200 {
        let message = text(&root, "Message");
        return Err(if root.get("Message").is_some() { message } else { "Yike live list request failed.".into() });
    }
    let Some(result) = root.get("Result").filter(|r| r.is_object()) else {
        return Ok((0, Vec::new()));
    };
    let games = result
        .get("list")
        .and_then(Value::as_array)
        .map(|list| list.iter().filter(|i| i.is_object()).map(live_game).collect())
        .unwrap_or_default();
    Ok((int(result, "since", 0), games))
}

/// What a detail or game-server response holds for a followed room (`parseSgf`'s `live` object):
/// the SGF (`Content`, else `sgf`, else `clean_sgf`), state 3 for a finished game, its result and
/// the names / komi / handicap the response gives beside the SGF.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LivePayload {
    pub sgf: String,
    pub finished: bool,
    pub result: String,
    pub black: Option<String>,
    pub white: Option<String>,
    pub komi: Option<f64>,
    pub handicap: Option<u32>,
    /// `hands_count` of a 对局室 response (`handleYikeUniteSgf` dedupe).
    pub hands: Option<i64>,
}

fn first_non_blank(values: &[String]) -> Option<String> {
    values.iter().find(|v| !blank(v)).cloned()
}

/// The `live` object of a response (`Result.live`, else `result`), read like `parseSgf`.
pub fn payload_of(live: &Value) -> LivePayload {
    let mut sgf = text(live, "Content");
    if blank(&sgf) {
        sgf = first_non_blank(&[text(live, "sgf"), text(live, "clean_sgf")]).unwrap_or_default();
    }
    let status_three = text(live, "Status") == "3" || text(live, "status") == "3" || int(live, "status", 0) == 3;
    LivePayload {
        sgf,
        finished: status_three,
        result: first_non_blank(&[text(live, "GameResult"), text(live, "game_result"), text(live, "resultDesc")])
            .unwrap_or_default(),
        black: first_non_blank(&[text(live, "BlackPlayer"), text(live, "black_name"), text(live, "blackName")]),
        white: first_non_blank(&[text(live, "WhitePlayer"), text(live, "white_name"), text(live, "whiteName")]),
        komi: live.get("komi").map(|_| float(live, "komi", f64::NAN)).filter(|k| k.is_finite()),
        handicap: live.get("handicap").map(|_| int(live, "handicap", 0).max(0) as u32),
        hands: live.get("hands_count").map(|_| int(live, "hands_count", -1)),
    }
}

/// Finds the `live` object of a response body like `parseSgf` (`Result.live`, else `result`).
pub fn live_object(body: &str) -> Option<Value> {
    let root: Value = serde_json::from_str(body).ok()?;
    if let Some(live) = root.get("Result").and_then(|r| r.get("live")).filter(|l| l.is_object()) {
        return Some(live.clone());
    }
    root.get("result").filter(|r| r.is_object()).cloned()
}

/// `parseLiveDetail`: `status` 0 (else the `message`) and a `result` object.
pub fn parse_detail(body: &str) -> Result<LivePayload, String> {
    let root: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    if int(&root, "status", -1) != 0 {
        return Err(if root.get("message").is_some() {
            text(&root, "message")
        } else {
            "Yike live detail request failed.".into()
        });
    }
    let result = root
        .get("result")
        .filter(|r| r.is_object())
        .ok_or_else(|| "Yike live detail response does not contain result.".to_string())?;
    Ok(payload_of(result))
}

/// `YikeSgfMainline.withoutVariations`: each game tree keeps only its first sub-tree; text that
/// does not parse as a complete tree comes back unchanged.
pub fn mainline_only(sgf: &str) -> String {
    let Some(start) = sgf.find('(') else { return sgf.to_string() };
    match tree(sgf.as_bytes(), start) {
        Some((text, _)) => text,
        None => sgf.to_string(),
    }
}

fn tree(sgf: &[u8], start: usize) -> Option<(String, usize)> {
    if sgf.get(start) != Some(&b'(') {
        return None;
    }
    let mut out = vec![b'('];
    let (mut in_value, mut escaping, mut copied) = (false, false, false);
    let mut i = start + 1;
    while i < sgf.len() {
        let ch = sgf[i];
        if in_value {
            out.push(ch);
            if escaping {
                escaping = false;
            } else if ch == b'\\' {
                escaping = true;
            } else if ch == b']' {
                in_value = false;
            }
            i += 1;
            continue;
        }
        match ch {
            b'[' => {
                in_value = true;
                out.push(ch);
                i += 1;
            }
            b'(' => {
                let (child, next) = tree(sgf, i)?;
                if !copied {
                    let inner = &child.as_bytes()[1..child.len() - 1];
                    out.extend_from_slice(inner);
                    copied = true;
                }
                i = next;
            }
            b')' => {
                out.push(ch);
                return Some((String::from_utf8_lossy(&out).into_owned(), i + 1));
            }
            _ => {
                out.push(ch);
                i += 1;
            }
        }
    }
    None
}

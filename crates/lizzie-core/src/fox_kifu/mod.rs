//! 野狐棋谱 (`GetFoxRequest`, `FoxKifuDownload`): the Fox kifu service's public queries and what
//! their replies mean. The platform adapter supplies the HTTP transport ([`Http`]) and runs
//! [`fetch`] off the workbench lock; the workbench owns the search session and opens a downloaded
//! record. A record is read by the ordinary SGF reader, after Java's Fox normalization.

pub mod normalize;

use serde_json::{Map, Value};

use crate::formats::{self, ReadError};
use crate::record::{GameRecord, RecordFormat};

const BASE_URL: &str = "https://h5.foxwq.com/yehuDiamond/chessbook_local";
const QUERY_USER_URL: &str = "https://newframe.foxwq.com/cgi/QueryUserInfoPanel";
const SGF_CGI_URLS: [&str; 2] = [
    "http://happyapp.huanle.qq.com/cgi-bin/CommonMobileCGI/TXWQFetchChess",
    "http://cgi.foxwq.com/cgi-bin/CommonMobileCGI/TXWQFetchChess",
];
/// The H5 pages' user agent (`MOBILE_USER_AGENT`).
pub const MOBILE_USER_AGENT: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 \
     (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1";
/// The record CGI's user agent (`CGI_USER_AGENT`).
pub const CGI_USER_AGENT: &str = "okhttp/3.12.12";

/// One request of the dialog (`GetFoxRequest.handleCommand`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// `user_name`: the nickname is always resolved to its account, even when it is all digits (a
    /// numeric nickname and a UID are different accounts), then that account's first page.
    Nickname { name: String },
    /// `account_uid`: the first page of an explicitly chosen UID; `nickname` only labels it until
    /// the games reveal the name.
    Account { uid: String, nickname: String },
    /// `uid`: the page after the game `last_code`.
    Page { uid: String, last_code: String },
    /// `chessid`: one game record.
    Record { chessid: String },
}

/// The transport, as Java's `httpRequest`: the body of any HTTP status, decoded as UTF-8; `Err`
/// with the failure text once its retries are spent.
pub trait Http {
    fn get(&mut self, url: &str, user_agent: &str) -> Result<String, String>;
    fn post_form(&mut self, url: &str, form: &[(&str, &str)], user_agent: &str) -> Result<String, String>;
}

/// One listed game (`FoxKifuDownload.handleChessList`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub chessid: String,
    /// `starttime` as Fox writes it.
    pub start_time: String,
    pub black_uid: String,
    pub white_uid: String,
    /// The first non-empty of the nickname fields; `None` shows 未知昵称.
    pub black_name: Option<String>,
    pub white_name: Option<String>,
    /// Fox rank codes (`blackdan`, `whitedan`).
    pub black_rank: i64,
    pub white_rank: i64,
    pub moves: i64,
    /// 1 Black, 2 White, otherwise no winner.
    pub winner: i64,
    /// Hundredths of a point; -1 resignation, -2 time, other negatives a plain win.
    pub point: i64,
    /// 0 area (目), 1 stones (子).
    pub rule: i64,
}

/// What a request brought back.
#[derive(Clone, Debug)]
pub enum Reply {
    /// A page of games. `uid` and `nickname` are the account the reply names (`fox_uid`,
    /// `fox_nickname`, `username`, `name`), empty when it names none.
    Games {
        uid: String,
        nickname: String,
        games: Vec<Game>,
    },
    /// A record, normalized like Java and read by the SGF reader.
    Record(Result<GameRecord, ReadError>),
    /// The request failed: the text shown after 获取棋谱失败,消息:, or the account does not exist.
    Failed { message: String, user_not_found: bool },
}

impl Reply {
    fn failed(message: impl Into<String>) -> Reply {
        Reply::Failed {
            message: message.into(),
            user_not_found: false,
        }
    }
}

/// Runs one request to its reply. `no_capture` is the editing setting a downloaded record is
/// read with (it also governs the record's normalization).
pub fn fetch(request: &Request, http: &mut dyn Http, no_capture: bool) -> Reply {
    match request {
        Request::Nickname { name } => nickname(name.trim(), http),
        Request::Account { uid, nickname } => {
            let uid = uid.trim();
            if uid.is_empty() || !uid.bytes().all(|b| b.is_ascii_digit()) {
                return Reply::failed(format!("invalid fox uid: {uid}"));
            }
            match http.get(&list_url(uid, "0"), MOBILE_USER_AGENT) {
                Ok(body) => games(&body, Some((uid, nickname.trim()))),
                Err(e) => Reply::failed(e),
            }
        }
        Request::Page { uid, last_code } => match http.get(&list_url(uid, last_code), MOBILE_USER_AGENT) {
            Ok(body) if body.trim().is_empty() => Reply::failed(EMPTY_RESPONSE),
            Ok(body) => games(&body, None),
            Err(e) => Reply::failed(e),
        },
        Request::Record { chessid } => record(chessid.trim(), http, no_capture),
    }
}

const EMPTY_RESPONSE: &str = "empty response from server";

/// `handleUserName` + `queryUserByName`.
fn nickname(name: &str, http: &mut dyn Http) -> Reply {
    if name.is_empty() {
        return Reply::failed("empty fox user");
    }
    let lookup = match http.get(
        &format!("{QUERY_USER_URL}?srcuid=0&username={}", form_encode(name)),
        MOBILE_USER_AGENT,
    ) {
        Ok(body) => body,
        Err(e) => return Reply::failed(e),
    };
    let account = match object(&lookup) {
        Ok(account) => account,
        Err(e) => return Reply::failed(e),
    };
    let result = if account.contains_key("result") {
        opt_int(&account, "result", -1)
    } else {
        opt_int(&account, "errcode", -1)
    };
    let uid = fox_uid(&account);
    if result != 0 {
        // Fox reports a missing account with a non-zero result and no UID; the codes vary with
        // the input, so the empty UID is the stable marker.
        if uid.is_empty() {
            return Reply::Failed {
                message: format!("Can't find a Fox account for nickname: {name}"),
                user_not_found: true,
            };
        }
        return Reply::failed(first_non_empty(&[
            &opt_string(&account, "resultstr"),
            &opt_string(&account, "errmsg"),
            &format!("Fox nickname lookup failed with result {result}"),
        ]));
    }
    if uid.is_empty() {
        return Reply::failed("Fox account was found, but the numeric UID was empty.");
    }
    let shown = first_non_empty(&[
        &opt_string(&account, "username"),
        &opt_string(&account, "name"),
        &opt_string(&account, "englishname"),
        name,
    ]);
    match http.get(&list_url(&uid, "0"), MOBILE_USER_AGENT) {
        Ok(body) => games(&body, Some((&uid, &shown))),
        Err(e) => Reply::failed(e),
    }
}

/// `fetchSgf`: the record CGIs in turn (a reply with result 0 and a record), then the H5 page.
fn record(chessid: &str, http: &mut dyn Http, no_capture: bool) -> Reply {
    let mut payload = None;
    if !chessid.is_empty() {
        for url in SGF_CGI_URLS {
            let Ok(body) = http.post_form(url, &[("chessid", chessid)], CGI_USER_AGENT) else {
                continue;
            };
            let usable = object(&body)
                .is_ok_and(|json| opt_int(&json, "result", -1) == 0 && !opt_string(&json, "chess").trim().is_empty());
            if usable {
                payload = Some(body);
                break;
            }
        }
    }
    let payload = match payload {
        Some(body) => body,
        None => match http.get(
            &format!("{BASE_URL}/YHWQFetchChess?chessid={}", form_encode(chessid)),
            MOBILE_USER_AGENT,
        ) {
            Ok(body) => body,
            Err(e) => return Reply::failed(e),
        },
    };
    if payload.trim().is_empty() {
        return Reply::failed(EMPTY_RESPONSE);
    }
    // FoxKifuDownload.handleResult: unreadable text is shown as it came.
    let Ok(json) = object(&payload) else {
        return Reply::failed(payload);
    };
    if opt_int(&json, "result", 0) != 0 {
        return Reply::failed(result_text(&json, &payload));
    }
    let sgf = match json.get("chess") {
        Some(Value::String(sgf)) if sgf.trim().is_empty() => sgf.clone(),
        Some(Value::String(sgf)) => normalize::normalize(sgf, no_capture),
        Some(_) => return Reply::failed(payload),
        None => return Reply::failed(result_text(&json, &payload)),
    };
    Reply::Record(formats::parse(RecordFormat::Sgf, &sgf, no_capture))
}

/// A game list as the dialog reads it (`handleResult`, `handleChessList`); `account` is the
/// identity `wrapChessListWithUserInfo` adds to a first page.
fn games(body: &str, account: Option<(&str, &str)>) -> Reply {
    let json = match object(body) {
        Ok(json) => json,
        // The first page is rewritten with its account (a JSON failure there is the error), a
        // later page is shown as it came.
        Err(e) if account.is_some() => return Reply::failed(e),
        Err(_) => return Reply::failed(body),
    };
    if opt_int(&json, "result", 0) != 0 {
        return Reply::failed(result_text(&json, body));
    }
    let Some(list) = json.get("chesslist") else {
        return Reply::failed(result_text(&json, body));
    };
    let Some(Ok(games)) = list
        .as_array()
        .map(|list| list.iter().map(game).collect::<Result<Vec<_>, ()>>())
    else {
        return Reply::failed(body);
    };
    // `fox_uid` / `fox_nickname` exist on a first page only; Fox's own name fields follow.
    let (uid, nickname) = account.unwrap_or_default();
    let nickname = first_non_empty(&[nickname, &opt_string(&json, "username"), &opt_string(&json, "name")]);
    let uid = uid.trim().to_string();
    Reply::Games { uid, nickname, games }
}

/// One `chesslist` entry; `Err` where Java's `getString` / `getInt` throws.
fn game(entry: &Value) -> Result<Game, ()> {
    let o = entry.as_object().ok_or(())?;
    let string = |key: &str| match o.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        _ => Err(()),
    };
    let int = |key: &str| o.get(key).and_then(as_int).ok_or(());
    let uid = |key: &str| {
        let text = opt_string(o, key).trim().to_string();
        if text.is_empty() {
            o.get(key).and_then(as_int).unwrap_or(0).to_string()
        } else {
            text
        }
    };
    let name = |keys: [&str; 4]| {
        keys.iter()
            .map(|k| opt_string(o, k).trim().to_string())
            .find(|s| !s.is_empty())
    };
    Ok(Game {
        start_time: string("starttime")?,
        black_uid: uid("blackuid"),
        white_uid: uid("whiteuid"),
        black_name: name(["blacknick", "blacknickname", "blackname", "blackenname"]),
        white_name: name(["whitenick", "whitenickname", "whitename", "whiteenname"]),
        black_rank: int("blackdan")?,
        white_rank: int("whitedan")?,
        chessid: string("chessid")?,
        moves: int("movenum")?,
        winner: int("winner")?,
        point: int("point")?,
        rule: int("rule")?,
    })
}

fn list_url(uid: &str, last_code: &str) -> String {
    let uid = form_encode(uid);
    format!(
        "{BASE_URL}/YHWQFetchChessList?srcuid=0&dstuid={uid}&type=1&lastcode={}&searchkey=&uin={uid}",
        form_encode(last_code)
    )
}

/// `URLEncoder.encode(text, UTF_8)`.
pub fn form_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'-' | b'*' | b'_' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn object(text: &str) -> Result<Map<String, Value>, String> {
    match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err("A JSONObject text must begin with '{'".into()),
        Err(e) => Err(e.to_string()),
    }
}

/// `optString(key, whole)`: the reply's own explanation, or the reply itself.
fn result_text(json: &Map<String, Value>, whole: &str) -> String {
    match json.get("resultstr") {
        None | Some(Value::Null) => whole.to_string(),
        Some(_) => opt_string(json, "resultstr"),
    }
}

/// `JSONObject.optString(key, "")`: strings as they are, other values in their JSON form.
fn opt_string(json: &Map<String, Value>, key: &str) -> String {
    match json.get(key) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// `JSONObject.optInt`: numbers (fractions truncated) and numeric strings.
fn opt_int(json: &Map<String, Value>, key: &str, default: i64) -> i64 {
    json.get(key).and_then(as_int).unwrap_or(default)
}

fn as_int(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_f64().filter(|f| f.is_finite()).map(|f| f.trunc() as i64)),
        Value::String(s) => {
            let s = s.trim();
            s.parse::<i64>().ok().or_else(|| {
                s.parse::<f64>()
                    .ok()
                    .filter(|f| f.is_finite())
                    .map(|f| f.trunc() as i64)
            })
        }
        _ => None,
    }
}

/// `GetFoxRequest.foxUid`: "0" is no account.
fn fox_uid(json: &Map<String, Value>) -> String {
    let uid = opt_string(json, "uid").trim().to_string();
    if uid == "0" {
        String::new()
    } else {
        uid
    }
}

fn first_non_empty(values: &[&str]) -> String {
    values
        .iter()
        .map(|v| v.trim())
        .find(|v| !v.is_empty())
        .unwrap_or_default()
        .to_string()
}

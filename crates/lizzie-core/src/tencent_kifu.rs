//! 腾讯棋谱 (`GetTencentRequest`, `TencentKifuDownload`): the Tencent Weiqi kifu service's public
//! queries and what their replies mean. The platform adapter supplies the HTTP transport
//! ([`Http`]) and runs [`fetch`] off the workbench lock; the workbench owns the search session and
//! opens a downloaded record. A record is read by the ordinary SGF reader after Java's small
//! clean-up ([`sanitize`]): its variations and comments stay.

use serde_json::{Map, Value};

use crate::formats::{self, ReadError};
use crate::kifu_web::{form_encode, object, opt_int, opt_string, result_text, Http};
use crate::record::{GameRecord, RecordFormat};

const CHESS_LIST_URL: &str = "https://cgi.huanle.qq.com/cgi-bin/CommonMobileCGI/TXWQFetchChessList";
const CHESS_DETAIL_URL: &str = "https://happyapp.huanle.qq.com/cgi-bin/CommonMobileCGI/TXWQFetchChess";
const SESSION: &str = "lizzieyzy-next";
/// `DEFAULT_FETCH_NUM`; a shorter list is the last one (`isComplete`).
pub const FETCH_NUM: usize = 100;
/// `USER_AGENT`.
pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/124.0 Safari/537.36";
const EMPTY_RESPONSE: &str = "empty response from server";

/// One request of the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// `search` (`last_code` "0") and `fetchNextPage`: the games of a nickname or numeric UID after
    /// the game `last_code`.
    List { query: String, last_code: String },
    /// `fetchKifu`: one game record.
    Record { chessid: String },
}

/// One listed game (`TencentKifuDownload.handleChessList`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub chessid: String,
    /// `starttime` as Tencent writes it; empty when only `gamestarttime` is given.
    pub start_time: String,
    /// `gamestarttime`, Unix seconds (0: none): shown in local time when `start_time` is empty.
    pub start_seconds: i64,
    pub black_uid: String,
    pub white_uid: String,
    /// The first non-empty of the nickname, English name and name; `None` shows 未知昵称.
    pub black_name: Option<String>,
    pub white_name: Option<String>,
    /// Tencent rank codes (`blackdan`, `whitedan`); 0 shows no rank.
    pub black_rank: i64,
    pub white_rank: i64,
    pub moves: i64,
    /// 1 Black, 2 White, otherwise no winner.
    pub winner: i64,
    /// Hundredths; -1 resignation, -2 time, other negatives a plain win.
    pub point: i64,
    /// 1 stones (子), otherwise points (目).
    pub rule: i64,
}

/// What a request brought back.
#[derive(Clone, Debug)]
pub enum Reply {
    /// A list: its games with a chess id, and how many entries the list held (fewer than
    /// [`FETCH_NUM`] is the last list).
    Games { games: Vec<Game>, fetched: usize },
    /// A record, cleaned like Java and read by the SGF reader.
    Record(Result<GameRecord, ReadError>),
    /// The request failed: the text shown after 获取腾讯棋谱失败，消息：.
    Failed { message: String },
}

fn failed(message: impl Into<String>) -> Reply {
    Reply::Failed { message: message.into() }
}

/// Runs one request to its reply. `no_capture` is the editing setting a record is read with.
pub fn fetch(request: &Request, http: &mut dyn Http, no_capture: bool) -> Reply {
    match request {
        Request::List { query, last_code } => {
            let query = query.trim();
            if query.is_empty() {
                return failed("empty tencent user");
            }
            match http.get(&list_url(query, last_code), USER_AGENT) {
                Ok(body) => games(&body),
                Err(e) => failed(e),
            }
        }
        Request::Record { chessid } => match http.get(&detail_url(chessid), USER_AGENT) {
            Ok(body) => record(&body, no_capture),
            Err(e) => failed(e),
        },
    }
}

/// `buildChessListUrl`: a numeric query is also the source UID.
pub fn list_url(query: &str, last_code: &str) -> String {
    let query = query.trim();
    let last_code = match last_code.trim() {
        "" => "0",
        code => code,
    };
    let src_uid = if !query.is_empty() && query.bytes().all(|b| b.is_ascii_digit()) {
        query
    } else {
        ""
    };
    format!(
        "{CHESS_LIST_URL}?type=7&lastCode={}&username={}&srcuid={}&txwqsession={SESSION}&fetchnum={FETCH_NUM}",
        form_encode(last_code),
        form_encode(query),
        form_encode(src_uid),
    )
}

/// `buildChessDetailUrl`.
pub fn detail_url(chessid: &str) -> String {
    format!("{CHESS_DETAIL_URL}?chessid={}", form_encode(chessid.trim()))
}

/// `isSuccess`: a present `result` or `ret` must be 0 (an unreadable one is -1).
fn success(json: &Map<String, Value>) -> bool {
    let code = |key| if json.contains_key(key) { opt_int(json, key, -1) } else { 0 };
    code("result") == 0 && code("ret") == 0
}

/// `handleResult` + `handleChessList` for a list reply. Java left the window waiting forever on an
/// empty body or a successful reply without a list; both are failures here.
fn games(body: &str) -> Reply {
    if body.trim().is_empty() {
        return failed(EMPTY_RESPONSE);
    }
    let json = match object(body) {
        Ok(json) => json,
        // `wrapListWithQuery` fails in the worker: its exception text is shown.
        Err(e) => return failed(e),
    };
    if !success(&json) {
        return failed(result_text(&json, body));
    }
    // `getJSONArray` / `getJSONObject` throw on anything else: the whole reply is shown.
    let Some(Value::Array(entries)) = json.get("chesslist") else {
        return failed(body);
    };
    let mut games = Vec::with_capacity(entries.len());
    for entry in entries {
        let Some(o) = entry.as_object() else {
            return failed(body);
        };
        if let Some(game) = game(o) {
            games.push(game);
        }
    }
    Reply::Games {
        games,
        fetched: entries.len(),
    }
}

/// One `chesslist` entry; an entry without a chess id is left out.
fn game(o: &Map<String, Value>) -> Option<Game> {
    let chessid = opt_string(o, "chessid").trim().to_string();
    if chessid.is_empty() {
        return None;
    }
    // `resolveUid`: the text, else a positive number.
    let uid = |key: &str| {
        let text = opt_string(o, key).trim().to_string();
        let n = opt_int(o, key, 0);
        if !text.is_empty() {
            text
        } else if n > 0 {
            n.to_string()
        } else {
            String::new()
        }
    };
    let name = |keys: [&str; 3]| {
        keys.iter()
            .map(|k| opt_string(o, k).trim().to_string())
            .find(|s| !s.is_empty())
    };
    Some(Game {
        chessid,
        start_time: opt_string(o, "starttime").trim().to_string(),
        start_seconds: opt_int(o, "gamestarttime", 0).max(0),
        black_uid: uid("blackuid"),
        white_uid: uid("whiteuid"),
        black_name: name(["blacknick", "blackenname", "blackname"]),
        white_name: name(["whitenick", "whiteenname", "whitename"]),
        black_rank: opt_int(o, "blackdan", 0),
        white_rank: opt_int(o, "whitedan", 0),
        moves: opt_int(o, "movenum", 0),
        winner: opt_int(o, "winner", 0),
        point: opt_int(o, "point", 0),
        rule: opt_int(o, "rule", 0),
    })
}

/// `normalizeTencentSgfPayload` + `handleResult` for a record reply.
fn record(body: &str, no_capture: bool) -> Reply {
    if body.trim().is_empty() {
        return failed(EMPTY_RESPONSE);
    }
    // Unreadable text is shown as it came.
    let Ok(json) = object(body) else {
        return failed(body);
    };
    if !success(&json) {
        return failed(result_text(&json, body));
    }
    let sgf = match json.get("chess") {
        Some(Value::String(sgf)) => sanitize(sgf),
        // `getString` throws on a non-text record; a reply without one left Java waiting.
        _ => return failed(body),
    };
    Reply::Record(formats::parse(RecordFormat::Sgf, &sgf, no_capture))
}

/// `sanitizeTencentSgf`: no byte-order mark or surrounding blanks, and no backslash outside a
/// property value (escapes inside values stay).
pub fn sanitize(sgf: &str) -> String {
    if sgf.trim().is_empty() {
        return sgf.to_string();
    }
    let text = sgf.replace('\u{feff}', "");
    let mut out = String::with_capacity(text.len());
    let mut in_value = false;
    let mut chars = text.trim().chars();
    while let Some(c) = chars.next() {
        if in_value {
            out.push(c);
            if c == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if c == ']' {
                in_value = false;
            }
            continue;
        }
        if c == '\\' {
            continue;
        }
        out.push(c);
        if c == '[' {
            in_value = true;
        }
    }
    out
}

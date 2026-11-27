//! OGS 棋谱 (`OgsGameClient`, `OgsSgfClient`): the public online-go.com REST API the OGS window
//! and 打开在线链接 read, and what its replies mean. No account is used: a player is looked up by
//! username or numeric ID, its completed games are listed 20 per page, newest first, and a game's
//! SGF is downloaded. Addresses are always built here from the API root; nothing the server sends
//! (such as its `next` page link) is followed.
//!
//! The platform adapter supplies the transport ([`Http`]: one GET, no redirects followed, only a
//! 200 reply's body read, up to [`MAX_BYTES`] + 1 bytes) and runs [`fetch`] off the workbench lock;
//! the workbench owns the window's session and opens a downloaded record through the ordinary open
//! path.
//!
//! Failures are told apart for the user (Java shows one message for all of them): the transport
//! failing ([`OgsFailure::Network`]), OGS limiting requests (HTTP 429, [`OgsFailure::RateLimited`]),
//! and any other reply that is not what was asked for ([`OgsFailure::Unavailable`]). An unknown
//! player is not a failure: a username without an exact match, or a numeric ID OGS answers with
//! 404, is [`Reply::NoPlayer`]. Like Java, nothing is retried.

use std::time::Duration;

use serde::Serialize;
use serde_json::{Map, Value};
use ts_rs::TS;

use crate::formats::{self, ReadError};
use crate::kifu_web::{as_int, form_encode, object, opt_string};
use crate::record::{GameRecord, RecordFormat};

/// `OgsGameClient.API`: the root every request is built from.
pub const API: &str = "https://online-go.com/api/v1/";
/// `OgsGameClient.PAGE_SIZE`.
pub const PAGE_SIZE: u32 = 20;
/// `OgsSgfClient.MAX_BYTES`: a longer reply is refused.
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
/// `readResponse`'s request headers.
pub const USER_AGENT: &str = "LizzieYzy-Next";
pub const ACCEPT: &str = "application/json,application/x-go-sgf,text/plain,*/*";
/// `setConnectTimeout(10000)` and `setReadTimeout(10000)`.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// One reply as the transport read it: the HTTP status and, for a 200 reply only, up to
/// [`MAX_BYTES`] + 1 body bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
}

/// The transport: one GET of `url` with [`USER_AGENT`] and [`ACCEPT`], redirects not followed.
/// Like `readResponse`, the status decides first: the body of any other reply than 200 is not
/// read, so a 429 or 404 whose body breaks off or never comes is still that status. `Err` is a
/// request that got no status (connection, timeout, proxy), or a 200 reply whose body could not
/// be read, with its reason.
pub trait Http {
    fn get(&mut self, url: &str) -> Result<Response, String>;
}

/// Why a request brought nothing usable back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum OgsFailure {
    /// No reply: the connection failed or timed out.
    Network,
    /// HTTP 429: OGS limits requests for now.
    RateLimited,
    /// Any other status, an oversized reply, or a reply that is not the JSON or SGF asked for.
    Unavailable,
}

/// `OgsGameClient.Player`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    pub id: u64,
    pub name: String,
}

/// `OgsGameClient.Game`: one listed completed game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub id: u64,
    /// `ended` as OGS writes it (an ISO-8601 instant); the window shows it in local time.
    pub ended: String,
    pub black: String,
    pub white: String,
    /// [`result`]: `B+R`, `W+T`, `B+6.5 points` (OGS's own outcome text), `Void`, ...
    pub result: String,
    /// `width×height`.
    pub size: String,
}

/// `OgsGameClient.GamePage`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GamePage {
    pub games: Vec<Game>,
    pub page: u32,
    /// At least 1, also for a player without games.
    pub pages: u32,
}

/// One request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// `findPlayer` then the first page of its games (`OgsKifuBrowser.search`).
    Search { query: String },
    /// `games(player, page)`.
    Page { player: Player, page: u32 },
    /// `sgf(gameId)` / `OgsSgfClient.download`: one game record. `game` is the ID's digits as a
    /// link wrote them (`OgsSgfClient` pastes the text, so an ID is not limited to an integer type).
    Record { game: String },
}

/// What a request brought back.
#[derive(Clone, Debug)]
pub enum Reply {
    /// The searched player and the first page of its games.
    Found {
        player: Player,
        page: GamePage,
    },
    /// No player has the searched username, or OGS knows no player with the searched ID.
    NoPlayer,
    Page(GamePage),
    /// A downloaded record, read by the SGF reader.
    Record(Result<GameRecord, ReadError>),
    Failed(OgsFailure),
}

/// Runs one request against the API root `base` (normally [`API`]) to its reply. `no_capture` is
/// the editing setting a record is read with.
pub fn fetch(request: &Request, base: &str, http: &mut dyn Http, no_capture: bool) -> Reply {
    match request {
        Request::Search { query } => search(query, base, http),
        Request::Page { player, page } => match games(player, *page, base, http) {
            Ok(page) => Reply::Page(page),
            Err(failure) => Reply::Failed(failure),
        },
        Request::Record { game } => match get(http, &sgf_url(base, game))
            .map_err(OgsFailure::from)
            .and_then(|body| sgf(&body))
        {
            Ok(text) => Reply::Record(formats::parse(RecordFormat::Sgf, &text, no_capture)),
            Err(failure) => Reply::Failed(failure),
        },
    }
}

/// `trim()`: Java drops every character up to U+0020 at both ends.
fn java_trim(text: &str) -> &str {
    text.trim_matches(|c: char| c <= ' ')
}

/// `[1-9][0-9]*`: a player ID rather than a username.
fn is_player_id(query: &str) -> bool {
    let mut bytes = query.bytes();
    matches!(bytes.next(), Some(b'1'..=b'9')) && bytes.all(|b| b.is_ascii_digit())
}

fn search(query: &str, base: &str, http: &mut dyn Http) -> Reply {
    let query = java_trim(query);
    if query.is_empty() {
        return Reply::NoPlayer;
    }
    let player = if is_player_id(query) {
        match get(http, &format!("{base}players/{query}/")) {
            Ok(body) => object(&body)
                .ok()
                .and_then(|json| player(&json))
                .ok_or(OgsFailure::Unavailable),
            // OGS answers an unknown ID with 404.
            Err(Fetched::Status(404)) => return Reply::NoPlayer,
            Err(e) => Err(e.into()),
        }
    } else {
        get(http, &format!("{base}players/?username={}", form_encode(query)))
            .map_err(OgsFailure::from)
            .and_then(|body| exact_username(&body, query))
    };
    let player = match player {
        Ok(Some(player)) => player,
        Ok(None) => return Reply::NoPlayer,
        Err(failure) => return Reply::Failed(failure),
    };
    match games(&player, 1, base, http) {
        Ok(page) => Reply::Found { player, page },
        Err(failure) => Reply::Failed(failure),
    }
}

/// `player(json)`: a positive `id` and a text `username`.
fn player(json: &Map<String, Value>) -> Option<Option<Player>> {
    let id = json.get("id").and_then(as_int).filter(|&id| id > 0)?;
    let name = json.get("username")?.as_str()?.to_string();
    Some(Some(Player { id: id as u64, name }))
}

/// The `results` entry whose username is the searched one, letter case aside; any other match
/// OGS returns is never taken.
fn exact_username(body: &str, query: &str) -> Result<Option<Player>, OgsFailure> {
    let json = object(body).map_err(|_| OgsFailure::Unavailable)?;
    let Some(Value::Array(results)) = json.get("results") else {
        return Err(OgsFailure::Unavailable);
    };
    for candidate in results {
        let candidate = candidate.as_object().ok_or(OgsFailure::Unavailable)?;
        let name = candidate
            .get("username")
            .and_then(Value::as_str)
            .ok_or(OgsFailure::Unavailable)?;
        if equals_ignore_case(query, name) {
            return player(candidate).ok_or(OgsFailure::Unavailable);
        }
    }
    Ok(None)
}

/// `String.equalsIgnoreCase`: character by character, equal in upper or lower case.
fn equals_ignore_case(a: &str, b: &str) -> bool {
    let (mut a, mut b) = (a.chars(), b.chars());
    loop {
        match (a.next(), b.next()) {
            (None, None) => return true,
            (Some(x), Some(y)) => {
                let same = x == y || x.to_uppercase().eq(y.to_uppercase()) || x.to_lowercase().eq(y.to_lowercase());
                if !same {
                    return false;
                }
            }
            _ => return false,
        }
    }
}

/// The address of `player`'s completed games, page `page`, newest first.
pub fn games_url(base: &str, player: u64, page: u32) -> String {
    format!("{base}players/{player}/games/?ended__isnull=false&ordering=-ended&page_size={PAGE_SIZE}&page={page}")
}

/// The address of a game's SGF download.
pub fn sgf_url(base: &str, game: &str) -> String {
    format!("{base}games/{game}/sgf")
}

/// `games(player, page)`.
fn games(player: &Player, page: u32, base: &str, http: &mut dyn Http) -> Result<GamePage, OgsFailure> {
    let body = get(http, &games_url(base, player.id, page.max(1)))?;
    game_page(&body, page.max(1)).ok_or(OgsFailure::Unavailable)
}

fn game_page(body: &str, page: u32) -> Option<GamePage> {
    let json = object(body).ok()?;
    let Value::Array(results) = json.get("results")? else {
        return None;
    };
    let games = results
        .iter()
        .map(|g| g.as_object().and_then(game))
        .collect::<Option<Vec<_>>>()?;
    let count = json
        .get("count")
        .and_then(as_int)
        .filter(|c| (0..=i64::from(i32::MAX)).contains(c))?;
    let pages = (count as u64).div_ceil(u64::from(PAGE_SIZE)).max(1) as u32;
    Some(GamePage { games, page, pages })
}

fn game(o: &Map<String, Value>) -> Option<Game> {
    let id = o.get("id").and_then(as_int).filter(|&id| id > 0)? as u64;
    let players = o.get("players")?.as_object()?;
    let name = |color: &str| Some(players.get(color)?.as_object()?.get("username")?.as_str()?.to_string());
    let side = |key: &str| o.get(key).and_then(as_int).and_then(|n| i32::try_from(n).ok());
    Some(Game {
        id,
        ended: o.get("ended")?.as_str()?.to_string(),
        black: name("black")?,
        white: name("white")?,
        result: result(o),
        size: format!("{}×{}", side("width")?, side("height")?),
    })
}

/// `OgsGameClient.result`: `Void` for an annulled game; with one loser, the winner and the
/// outcome (`R` resignation, `T` timeout, otherwise OGS's text such as the points); without one,
/// the outcome as OGS writes it.
pub fn result(game: &Map<String, Value>) -> String {
    let flag = |key: &str| match game.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s.eq_ignore_ascii_case("true"),
        _ => false,
    };
    let outcome = opt_string(game, "outcome");
    if flag("annulled") {
        return "Void".into();
    }
    let (black_lost, white_lost) = (flag("black_lost"), flag("white_lost"));
    if black_lost == white_lost {
        return outcome;
    }
    let outcome = if outcome.eq_ignore_ascii_case("Resignation") {
        "R"
    } else if outcome.eq_ignore_ascii_case("Timeout") {
        "T"
    } else {
        &outcome
    };
    format!("{}{outcome}", if black_lost { "W+" } else { "B+" })
}

/// `OgsSgfClient.download`'s check of the text: trimmed, without a byte-order mark, and starting
/// `(` `;` and ending `)` — a login page, an error object or a cut-off record never reaches the
/// SGF reader.
fn sgf(body: &str) -> Result<String, OgsFailure> {
    let mut text = body.trim();
    if let Some(rest) = text.strip_prefix('\u{feff}') {
        text = rest.trim_start();
    }
    let opened = text.strip_prefix('(').is_some_and(|rest| {
        rest.trim_start_matches([' ', '\t', '\n', '\u{b}', '\u{c}', '\r'])
            .starts_with(';')
    });
    if opened && text.ends_with(')') {
        Ok(text.to_string())
    } else {
        Err(OgsFailure::Unavailable)
    }
}

enum Fetched {
    Transport,
    Status(u16),
    TooLarge,
}

impl From<Fetched> for OgsFailure {
    fn from(f: Fetched) -> OgsFailure {
        match f {
            Fetched::Transport => OgsFailure::Network,
            Fetched::Status(429) => OgsFailure::RateLimited,
            Fetched::Status(_) | Fetched::TooLarge => OgsFailure::Unavailable,
        }
    }
}

/// `readResponse`: only a 200 reply of at most [`MAX_BYTES`] counts; its body is UTF-8 (lossy).
fn get(http: &mut dyn Http, url: &str) -> Result<String, Fetched> {
    let reply = http.get(url).map_err(|_| Fetched::Transport)?;
    if reply.status != 200 {
        return Err(Fetched::Status(reply.status));
    }
    if reply.body.len() > MAX_BYTES {
        return Err(Fetched::TooLarge);
    }
    Ok(String::from_utf8_lossy(&reply.body).into_owned())
}

/// `OgsSgfClient.sgfUri`: the game (its ID's digits) an OGS game page or SGF download address
/// names. The trimmed text must be a URI `URI.create` accepts as a whole (any invalid escape or
/// character, in the query and fragment too, makes it none); then only `http` or `https` on
/// `online-go.com` or `www.online-go.com` without user info or a port number, with the decoded
/// path `/game/<id>`, `/api/v1/games/<id>/sgf` or `/api/v1/games/<id>/sgf/<n>.sgf` (a trailing
/// `/` allowed). Anything else is not an OGS link and follows the ordinary 打开在线链接 path.
pub fn link_game(input: &str) -> Option<String> {
    let text = java_trim(input);
    let (scheme, rest) = text.split_once(':')?;
    if !(scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http")) {
        return None;
    }
    let rest = rest.strip_prefix("//")?;
    let (authority, rest) = rest.split_at(rest.find(['/', '?', '#']).unwrap_or(rest.len()));
    // `getHost`, `getUserInfo() == null`, `getPort() == -1`: a `:` without digits is no port.
    let host = authority.strip_suffix(':').unwrap_or(authority);
    if !(host.eq_ignore_ascii_case("online-go.com") || host.eq_ignore_ascii_case("www.online-go.com")) {
        return None;
    }
    let (rest, fragment) = rest.split_once('#').unwrap_or((rest, ""));
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
    if !(uri_text(path, ":@&=+$,;/") && uri_text(query, ";/?:@&=+$,[]") && uri_text(fragment, ";/?:@&=+$,[]")) {
        return None;
    }
    // `getPath` is the decoded path; a byte sequence that is no UTF-8 cannot match.
    let path = percent_encoding::percent_decode_str(path).decode_utf8().ok()?;
    let path = path.strip_suffix('/').unwrap_or(&path);
    let id = |text: &str| {
        let digits = text.as_bytes().first().is_some_and(|b| (b'1'..=b'9').contains(b))
            && text.bytes().all(|b| b.is_ascii_digit());
        digits.then(|| text.to_string())
    };
    if let Some(game) = path.strip_prefix("/game/") {
        return id(game);
    }
    let rest = path.strip_prefix("/api/v1/games/")?;
    let (game, tail) = rest.split_once('/')?;
    let game = id(game)?;
    match tail.strip_prefix("sgf")? {
        "" => Some(game),
        file => {
            let n = file.strip_prefix('/')?.strip_suffix(".sgf")?;
            id(n).map(|_| game)
        }
    }
}

/// Whether `text` is all characters `java.net.URI` takes in a path (`allowed` its punctuation
/// `:@&=+$,;/`) or a query or fragment (`;/?:@&=+$,[]`): letters, digits, `-_.!~*'()`, `%` with
/// two hex digits, and any other non-ASCII character that is neither a space nor a control.
fn uri_text(text: &str, allowed: &str) -> bool {
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        let ok = match c {
            '%' => (0..2).all(|_| chars.next().is_some_and(|h| h.is_ascii_hexdigit())),
            c if c.is_ascii() => c.is_ascii_alphanumeric() || "-_.!~*'()".contains(c) || allowed.contains(c),
            c => !c.is_whitespace() && !c.is_control(),
        };
        if !ok {
            return false;
        }
    }
    true
}

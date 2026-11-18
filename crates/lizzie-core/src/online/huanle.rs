//! 野狐分享 / 腾讯欢乐 broadcasts (`OnlineDialog.req`, `req0`, `parseReq`, `processFrag`, types
//! 3 and 4): the huanle binary WebSocket protocol, and the record download when a game ends.
//!
//! The session keeps its own record of the broadcast game (Java's `history`, OD 111) and reports
//! it whole after every change. Java places handicap stones as black moves (OD 2065-2128); here
//! they are root setup stones (`AB`, `HA`), and Java's move numbers, which count those stones,
//! are mapped onto the record by `Game::offset`.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use super::{Connect, Frame, HttpNeed, HttpReply, OnlineText, SourceUpdate, Step};
use crate::formats;
use crate::record::{
    Applied, BoardSize, DocumentEdit, GameRecord, NodeId, NodeKind, PlayRules, Point, RecordFormat, SourceAction, Stone,
    ROOT,
};

/// Type 3 broadcast socket, Java byte array `b` (OD 136-139).
const BROADCAST_URL: &str = "wss://hlws.huanle.qq.com/wqbroadcastlotus";
/// Type 4 broadcast socket, Java byte array `b2` (OD 140-144).
const HALL_URL: &str = "ws://wshall.huanle.qq.com/NewEagleEyeLotus";
/// Record download, Java byte array `b3` (OD 145-150) followed by the `chessid` (`getSgf`).
const RECORD_URL: &str = "https://weiqi.qq.com/openqipu/getqipu?callback=jQuery1&gamecode=";

/// `req0`'s JSONP RequestBroadcast read (type 4).
const PURPOSE_REQUEST: u32 = 1;
/// `getSgf`'s record download.
const PURPOSE_RECORD: u32 = 2;

/// `proc`: a 刷新 interval that is not positive means 10 seconds (OD 835-836).
const DEFAULT_REFRESH: Duration = Duration::from_secs(10);
/// `scheduleAtFixedRate(..., 1, refreshTime, SECONDS)`: the first follow request (OD 1913-1944).
const FIRST_FOLLOW: Duration = Duration::from_secs(1);

/// Message ids (`req1`/`req2` `msgID`, `parseReq` OD 1862-2028).
const MSG_BROADCAST: i16 = 23406;
const MSG_PUSH: i16 = 23407;
const MSG_FOLLOW: i16 = 23413;
const MSG_TRANSPARENT: i16 = 23414;

/// Fragment types `processFrag` handles (OD 2038-2297).
const FRAG_NEW_GAME: u32 = 20032;
const FRAG_SWAP_PLAYERS: u32 = 4116;
const FRAG_MOVE: u32 = 7005;
const FRAG_PASS: u32 = 7045;
const FRAG_COMMENT: u32 = 8005;
const FRAG_BRANCH: u32 = 8185;
const FRAG_RESULT: u32 = 7185;
const FRAG_RESULT_OTHER: u32 = 7186;

/// Handicap stones Java places on 19×19 (OD 2065-2128), in its order; `(x, y)` from the top left.
fn handicap_points(handicap: i64) -> &'static [(u16, u16)] {
    match handicap {
        2 => &[(15, 3), (3, 15)],
        3 => &[(3, 3), (15, 3), (3, 15)],
        4 => &[(3, 3), (3, 15), (15, 3), (15, 15)],
        5 => &[(3, 3), (3, 15), (15, 3), (15, 15), (9, 9)],
        6 => &[(3, 3), (3, 15), (15, 3), (15, 15), (3, 9), (15, 9)],
        7 => &[(3, 3), (3, 15), (15, 3), (15, 15), (3, 9), (15, 9), (9, 9)],
        8 => &[(3, 3), (3, 15), (15, 3), (15, 15), (9, 3), (9, 15), (3, 9), (15, 9)],
        9 => &[(3, 3), (3, 15), (15, 3), (15, 15), (9, 3), (9, 15), (3, 9), (15, 9), (9, 9)],
        _ => &[],
    }
}

/// One followed broadcast.
pub struct Session {
    /// Type 3 (socket `b`, three more ints at the end of replies) rather than type 4 (`b2`).
    fox: bool,
    /// Java `queryMap` (first value per key); `req0` replaces the broadcast keys.
    query: Vec<(String, String)>,
    refresh: Duration,
    /// Java `chineseRule == 1` (OD 130, 1701-1703).
    chinese_rule: bool,
    /// Java `chineseFlag` (OD 129, 2135-2137): results are counted in stones (子).
    chinese_flag: bool,
    /// Java `seqs`.
    seq: i32,
    /// Java `client.isOpen()`.
    open: bool,
    /// Java `done`: no follow requests, no reconnect.
    done: bool,
    /// Next run of the fixed-rate follow task (Java `schedule`).
    due: Option<Instant>,
    game: Option<Game>,
}

/// Java `history` with the dialog's `blackPlayer`/`whitePlayer` and the game information
/// `processFrag` sets.
struct Game {
    record: GameRecord,
    /// Handicap stones Java would have placed as moves: Java move number − offset = record move
    /// number.
    offset: i64,
    /// `GameInfo.getHandicap()` as `processFrag` set it (only above 1, OD 2063).
    handicap: i64,
    black: String,
    white: String,
    komi: f64,
}

impl Game {
    fn update(&self) -> SourceUpdate {
        SourceUpdate::Game {
            record: Box::new(self.record.clone()),
            names: Some((self.black.clone(), self.white.clone())),
            komi: Some(self.komi),
            handicap: u32::try_from(self.handicap).ok().filter(|&h| h > 1),
        }
    }

    /// `Board.isValid` on the record's board.
    fn point(&self, (x, y): (i64, i64)) -> Option<Point> {
        let size = self.record.size;
        (x >= 0 && y >= 0 && x < i64::from(size.width) && y < i64::from(size.height))
            .then_some(Point { x: x as u16, y: y as u16 })
    }

    /// `getLastMoveColor()` at `node`: the handicap stones count as black moves.
    fn last_color(&self, node: NodeId) -> Option<Stone> {
        self.record
            .node(node)
            .kind
            .action_color()
            .or((self.offset > 0).then_some(Stone::Black))
    }
}

impl Session {
    /// Type 3 (`gameid` + `createtime`) connects at once (`req(true)`, OD 905-907); type 4
    /// (`gametag` + `uin`) first reads `request_url` (`req0`, OD 908-910). `refresh` is the 刷新
    /// interval of the follow requests.
    pub fn new(query: &[(String, String)], request_url: Option<&str>, refresh: Duration) -> (Self, Step) {
        let session = Session {
            fox: request_url.is_none(),
            query: query.to_vec(),
            refresh: if refresh.is_zero() { DEFAULT_REFRESH } else { refresh },
            chinese_rule: true,
            chinese_flag: false,
            seq: 0,
            open: false,
            done: false,
            due: None,
            game: None,
        };
        let step = match request_url {
            None => session.connect(Duration::ZERO),
            Some(url) => Step {
                fetch: vec![HttpNeed {
                    purpose: PURPOSE_REQUEST,
                    url: url.to_string(),
                    headers: Vec::new(),
                }],
                ..Step::default()
            },
        };
        (session, step)
    }

    /// The reply to `req0` (connects the hall socket) or to `getSgf` (the final record).
    pub fn on_http(&mut self, purpose: u32, reply: &HttpReply) -> Step {
        match purpose {
            PURPOSE_REQUEST => self.on_request(reply),
            PURPOSE_RECORD => self.on_record(reply),
            _ => Step::default(),
        }
    }

    /// `onOpen` (OD 1761-1772): msg 23406 asks for the broadcast.
    pub fn on_open(&mut self) -> Step {
        self.open = true;
        // `req` resets `seqs` before connecting (OD 1751); `onOpen` sends `++seqs`.
        self.seq = 1;
        Step {
            send: vec![Frame::Binary(self.broadcast_request())],
            ..Step::default()
        }
    }

    /// `onMessage(ByteBuffer)` → `parseReq` (OD 1787-1791, 1859-2029). A message Java's decoder
    /// would throw on is dropped whole.
    pub fn on_binary(&mut self, data: &[u8]) -> Step {
        let mut step = Step::default();
        let Ok(message) = parse_message(data, self.fox) else {
            return step;
        };
        match message {
            Message::Broadcast { result, frags, complete } => {
                self.process(&frags, &mut step);
                // OD 1891-1894: the game is over; its record is downloaded.
                if result == 23409 || result == 23412 {
                    self.done = true;
                    self.due = None;
                    let chessid = self.query_str("chessid");
                    if !chessid.trim().is_empty() {
                        step.fetch.push(HttpNeed {
                            purpose: PURPOSE_RECORD,
                            url: format!("{RECORD_URL}{chessid}"),
                            headers: Vec::new(),
                        });
                    }
                }
                // OD 1912-1945: the follow requests start once, 1 s after the first full reply.
                if complete && !self.done && self.due.is_none() {
                    self.due = Some(Instant::now() + FIRST_FOLLOW);
                }
            }
            Message::Push(frags) => self.process(&frags, &mut step),
            Message::Other => {}
        }
        step
    }

    /// The socket ended. Java `onClose` does nothing; the next fixed-rate run finds the client
    /// closed, cancels itself and, unless done, connects again (`reReq`, OD 1934-1939) — at most
    /// one 刷新 interval after the close. The reconnect here waits that interval.
    pub fn on_closed(&mut self, error: Option<&str>) -> Step {
        let _ = error;
        self.open = false;
        self.due = None;
        if self.done {
            Step::default()
        } else {
            self.connect(self.refresh)
        }
    }

    /// The fixed-rate follow task (OD 1913-1944): msg 23413 every 刷新 interval while open.
    pub fn tick(&mut self, now: Instant) -> Step {
        let Some(due) = self.due.filter(|&due| now >= due) else {
            return Step::default();
        };
        if !self.open || self.done {
            self.due = None;
            return Step::default();
        }
        let mut next = due + self.refresh;
        while next <= now {
            next += self.refresh;
        }
        self.due = Some(next);
        self.seq += 1;
        Step {
            send: vec![Frame::Binary(self.follow_request())],
            ..Step::default()
        }
    }

    pub fn next_due(&self) -> Option<Instant> {
        self.due
    }

    /// `req` (OD 1750-1796): type 3 uses `b`, type 4 `b2`.
    fn connect(&self, after: Duration) -> Step {
        Step {
            connect: Some(Connect {
                url: (if self.fox { BROADCAST_URL } else { HALL_URL }).to_string(),
                after,
            }),
            ..Step::default()
        }
    }

    /// `req0` (OD 1686-1736): the JSONP reply names the broadcast, then `req(true)`. Java ignores
    /// a reply it cannot read; here the follow stops with the dialog's invalid-link text. A failed
    /// HTTP status stops it whatever its body says (Java read the error body as the broadcast).
    fn on_request(&mut self, reply: &HttpReply) -> Step {
        let body = match reply_body(reply) {
            Ok(body) => body,
            Err(message) => {
                return Step::update(SourceUpdate::Stopped(OnlineText::with("Online.status.pullError", [message])));
            }
        };
        let Some(o) = jsonp_object(body) else {
            return Step::update(SourceUpdate::Stopped(OnlineText::new("OnlineDialog.lblError.text")));
        };
        if opt_int(&o, "result") == 0 && opt_int(&o, "ResultID") == 0 {
            self.chinese_rule = false;
        }
        self.set_query("gameid", "369".to_string());
        self.set_query("showtype", opt_string(&o, "ShowType"));
        self.set_query("showid", opt_string(&o, "ShowID"));
        self.set_query("createtime", opt_string(&o, "CreateTime"));
        self.connect(Duration::ZERO)
    }

    /// `getSgf` → `refresh` → `parseSgf` (OD 1674-1684, 1117-1126): `jQuery1("<url-encoded
    /// SGF>"...`, read like a first sync (players and komi from the record, OD 1157-1160,
    /// 1218-1224). A failure, a failed HTTP status included, shows `YikeLiveDialog.syncFailed`
    /// like `parseSgf` (OD 1250-1261).
    fn on_record(&mut self, reply: &HttpReply) -> Step {
        let text = match reply_body(reply) {
            Ok(body) => record_sgf(body),
            Err(message) => {
                return Step::update(SourceUpdate::Status(OnlineText::with("YikeLiveDialog.syncFailed", [message])));
            }
        };
        let Some(record) = text.and_then(|t| formats::parse(RecordFormat::Sgf, &t, false).ok()) else {
            return Step::update(SourceUpdate::Status(OnlineText::new("YikeLiveDialog.syncFailed")));
        };
        let komi = record.komi_declared.then_some(record.info.komi);
        Step::update(SourceUpdate::Game {
            record: Box::new(record),
            names: None,
            komi,
            handicap: None,
        })
    }

    /// `processFrag` (OD 2031-2311). The record is reported once after the fragments, like
    /// Java's single `sync` at the end; comments, branches and the result follow it.
    fn process(&mut self, frags: &[Fragment], step: &mut Step) {
        let mut changed = false;
        let mut after = Vec::new();
        for f in frags {
            let outcome = match f.kind {
                FRAG_NEW_GAME => self.new_game(&f.line),
                FRAG_SWAP_PLAYERS => Some(self.swap_players()),
                FRAG_MOVE => self.game.as_mut().map(|g| g.play(&f.line)),
                FRAG_PASS => self.game.as_mut().map(Game::pass),
                FRAG_COMMENT => self.comment(&f.line, &mut after),
                FRAG_BRANCH => self.game.as_ref().map(|g| {
                    after.extend(g.branch(&f.line));
                    false
                }),
                FRAG_RESULT | FRAG_RESULT_OTHER => self.finish(f, step, &mut after),
                // 7198 (clock times) is only kept for a commented-out display (OD 2222-2238).
                _ => Some(false),
            };
            // `None`: Java leaves the loop (`break` on a size of 0, OD 2146-2148) or throws (no
            // game yet; `result` of a 7186, which has no decoder).
            match outcome {
                Some(c) => changed |= c,
                None => break,
            }
        }
        if changed {
            if let Some(g) = &self.game {
                step.updates.push(g.update());
            }
        }
        step.updates.append(&mut after);
    }

    /// 20032 (OD 2038-2148): a new game with its size, handicap stones, players and komi.
    fn new_game(&mut self, line: &Fields) -> Option<bool> {
        let Some(Wire::Msg(info)) = line.get(&6) else {
            return None;
        };
        let size = field_int(info, 16);
        let handicap = field_int(info, 4);
        // `SetBoardSize`'s limit bounds what a broadcast can open.
        if !(1..=52).contains(&size) {
            return None;
        }
        // AAA309 is Black, AAA308 White: the Chinese name (AAA225) when AAA227 is 86, else AAA224.
        let player = |field: u64| match line.get(&field) {
            Some(Wire::Msg(p)) => field_text(p, if field_int(p, 4) == 86 { 2 } else { 1 }),
            _ => String::new(),
        };
        let black = player(8);
        let white = player(7);
        let a5 = field_int(info, 5);
        let komi = if handicap == 0 && a5 == 0 {
            6.5
        } else if field_int(info, 10) == 1 && self.chinese_rule {
            self.chinese_flag = true;
            a5 as f64 / 100.0 * 2.0
        } else {
            a5 as f64 / 100.0
        };
        let side = size as u16;
        let mut record = GameRecord::empty(BoardSize { width: side, height: side }, komi);
        let stones = if size == 19 { handicap_points(handicap) } else { &[] };
        for &(x, y) in stones {
            let _ = record.apply(&DocumentEdit::SetupStone {
                point: Point { x, y },
                stone: Some(Stone::Black),
            });
        }
        if !stones.is_empty() {
            let _ = record.apply(&DocumentEdit::SetupSideToPlay { color: Stone::White });
        }
        let handicap = if handicap > 1 { handicap } else { 0 };
        if handicap > 0 {
            let _ = record.apply(&DocumentEdit::SetHandicap {
                handicap: handicap as u32,
            });
        }
        let _ = record.apply(&DocumentEdit::SetPlayers {
            black: black.clone(),
            white: white.clone(),
        });
        self.game = Some(Game {
            record,
            offset: stones.len() as i64,
            handicap,
            black,
            white,
            komi,
        });
        Some(true)
    }

    /// 4116 (OD 2149-2161): the players change sides.
    fn swap_players(&mut self) -> bool {
        let Some(g) = &mut self.game else {
            return false;
        };
        std::mem::swap(&mut g.black, &mut g.white);
        let _ = g.record.apply(&DocumentEdit::SetPlayers {
            black: g.black.clone(),
            white: g.white.clone(),
        });
        true
    }

    /// 8005 (OD 2239-2247): a comment for the node at a Java move number.
    fn comment(&self, line: &Fields, after: &mut Vec<SourceUpdate>) -> Option<bool> {
        let num = field_int(line, 4);
        let text = field_text(line, 5);
        if num > 0 && !text.trim().is_empty() {
            let g = self.game.as_ref()?;
            let number = num - g.offset;
            if number >= 0 {
                let last = main_line(&g.record).len() as i64;
                after.push(SourceUpdate::Comment {
                    move_number: number.min(last) as u32,
                    text,
                });
            }
        }
        Some(false)
    }

    /// 7185/7186 (OD 2283-2297): the game is over; the socket closes and the result is set.
    fn finish(&mut self, f: &Fragment, step: &mut Step, after: &mut Vec<SourceUpdate>) -> Option<bool> {
        self.done = true;
        self.due = None;
        if self.open {
            step.close = true;
        }
        if f.kind == FRAG_RESULT_OTHER {
            // `result(7186, null)` throws: 7186 has no decoder (OD 2397-2414, 2362-2365).
            return None;
        }
        let result = result_text(&f.line, self.chinese_flag);
        // `history.next()` throws before any game.
        self.game.as_ref()?;
        after.push(SourceUpdate::Finished { result, set_result: true });
        Some(false)
    }

    fn query_str(&self, key: &str) -> &str {
        self.query.iter().find(|(k, _)| k == key).map_or("", |(_, v)| v.as_str())
    }

    /// `Utils.intOfMap`: the first value as an int, else 0.
    fn query_int(&self, key: &str) -> i32 {
        self.query_str(key).parse().unwrap_or(0)
    }

    /// `queryMap.put`: the key's values are replaced.
    fn set_query(&mut self, key: &str, value: String) {
        self.query.retain(|(k, _)| k != key);
        self.query.push((key.to_string(), value));
    }

    /// `req1(90, seq, 23406, gameid, showtype, showid, createtime)` (OD 1798-1831).
    fn broadcast_request(&self) -> Vec<u8> {
        let mut b = self.request_head(90, MSG_BROADCAST, 60);
        put_i32(&mut b, 0);
        put_i32(&mut b, -1);
        put_i32(&mut b, 0);
        put_i16(&mut b, 0);
        put_i32(&mut b, 3601);
        put_i32(&mut b, self.query_int("createtime"));
        for _ in 0..4 {
            put_i32(&mut b, 0);
        }
        put_i32(&mut b, 1);
        b
    }

    /// `req2(54, seq, 23413, ...)` (OD 1833-1857); the create time is not sent.
    fn follow_request(&self) -> Vec<u8> {
        let mut b = self.request_head(54, MSG_FOLLOW, 24);
        put_i16(&mut b, 0);
        put_i32(&mut b, 3601);
        b
    }

    /// The common start of `req1` and `req2`, through the show id.
    fn request_head(&self, len: i16, msg: i16, body: i16) -> Vec<u8> {
        let mut b = Vec::with_capacity(len as usize);
        put_i16(&mut b, len);
        put_i16(&mut b, 1);
        put_i32(&mut b, self.seq);
        put_i16(&mut b, -4);
        put_i32(&mut b, 50000);
        b.extend_from_slice(&[0, 0]);
        put_i16(&mut b, msg);
        put_i16(&mut b, 0);
        put_i32(&mut b, 1000);
        b.extend_from_slice(&[0, 234]);
        put_i16(&mut b, 0);
        put_i16(&mut b, 0);
        put_i16(&mut b, body);
        put_i32(&mut b, 0);
        put_i32(&mut b, self.query_int("gameid"));
        put_i32(&mut b, self.query_int("showtype"));
        put_i32(&mut b, self.query_int("showid"));
        b
    }
}

impl Game {
    /// 7005 (OD 2162-2218): a move or pass at a Java move number; a number already played cuts
    /// the line back there first, keeping that move when it is the same.
    fn play(&mut self, line: &Fields) -> bool {
        let main = main_line(&self.record);
        let moves = main.len() as i64 + self.offset;
        let mut num = field_int(line, 8) + self.handicap;
        if num == 0 {
            num = moves + 1;
        }
        let color = if field_int(line, 6) & 3 == 1 { Stone::White } else { Stone::Black };
        let index = field_int(line, 4);
        let point = self.point(as_coord(if color == Stone::Black { index } else { index - 1024 }));
        let kind = match point {
            Some(point) => NodeKind::Move { color, point },
            None => NodeKind::Pass { color },
        };
        let mut changed = false;
        let mut at = main.last().copied().unwrap_or(ROOT);
        if num <= moves && !main.is_empty() {
            let k = (num - self.offset).clamp(1, main.len() as i64) as usize;
            at = if k >= 2 { main[k - 2] } else { ROOT };
            let keep = self.record.node(main[k - 1]).kind == kind;
            if let Some(&cut) = main.get(if keep { k } else { k - 1 }) {
                changed |= self.record.apply(&DocumentEdit::Remove { node: cut }).is_ok_and(|a| a.changed);
            }
        }
        let edit = match point {
            Some(point) => DocumentEdit::Play {
                at,
                color,
                point,
                alternate: false,
                rules: PlayRules::default(),
            },
            None => DocumentEdit::Pass { at, color },
        };
        changed | self.record.apply(&edit).is_ok_and(|a| a.changed)
    }

    /// 7045 (OD 2219-2221): a pass by the side after the last move.
    fn pass(&mut self) -> bool {
        let at = main_line(&self.record).last().copied().unwrap_or(ROOT);
        let color = next_color(self.last_color(at));
        self.record
            .apply(&DocumentEdit::Pass { at, color })
            .is_ok_and(|a| a.changed)
    }

    /// 8185 (OD 2248-2282): a variation from the node at Java move number AAA20 − 1, its first
    /// move carrying the commentary. Played on the record cut there, sent as its actions.
    fn branch(&self, line: &Fields) -> Option<SourceUpdate> {
        let Some(Wire::Msg(b)) = line.get(&6) else {
            return None;
        };
        let moves = match b.get(&1) {
            Some(Wire::Ints(v)) => v.as_slice(),
            _ => &[],
        };
        let move_num = field_int(b, 3) - 1;
        if move_num <= 0 || moves.is_empty() {
            return None;
        }
        let number = move_num - self.offset;
        if number < 0 {
            return None;
        }
        let main = main_line(&self.record);
        let number = (number as usize).min(main.len());
        let mut rec = self.record.clone();
        if let Some(&next) = main.get(number) {
            let _ = rec.apply(&DocumentEdit::Remove { node: next });
        }
        let start = if number == 0 { ROOT } else { main[number - 1] };
        let mut at = start;
        let mut last = self.last_color(start);
        let comment = field_text(b, 4);
        for (i, &index) in moves.iter().enumerate() {
            let color = next_color(last);
            let index = i64::from(index as i32);
            let Some(point) = self.point(as_coord(if color == Stone::Black { index } else { index - 1024 }))
            else {
                break;
            };
            let Ok(Applied { node: Some(node), .. }) = rec.apply(&DocumentEdit::Play {
                at,
                color,
                point,
                alternate: false,
                rules: PlayRules::default(),
            }) else {
                break;
            };
            if i == 0 {
                let _ = rec.apply(&DocumentEdit::SetComment {
                    node,
                    comment: format!("{comment}\n"),
                });
            }
            at = node;
            last = Some(color);
        }
        let path = rec.path_to(at);
        let from = path.iter().position(|&n| n == start).map_or(0, |i| i + 1);
        let actions: Vec<SourceAction> = path[from..]
            .iter()
            .map(|&n| SourceAction { kind: rec.node(n).kind, comment: rec.node(n).comment.clone() })
            .collect();
        (!actions.is_empty()).then_some(SourceUpdate::Branch { move_number: number as u32, actions })
    }
}

/// `history.getLastMoveColor() == WHITE ? BLACK : WHITE`.
fn next_color(last: Option<Stone>) -> Stone {
    if last == Some(Stone::White) { Stone::Black } else { Stone::White }
}

/// The record's main line after the root.
fn main_line(rec: &GameRecord) -> Vec<NodeId> {
    let mut line = Vec::new();
    let mut id = ROOT;
    while let Some(next) = rec.main_child(id) {
        line.push(next);
        id = next;
    }
    line
}

/// `asCoord` (OD 2378-2386): `1024 + y·32 + x`; anything lower is the top-left point.
fn as_coord(index: i64) -> (i64, i64) {
    if index >= 1024 {
        let i = index - 1024;
        (i % 32, i / 32)
    } else {
        (0, 0)
    }
}

/// `result` (OD 2335-2376) for 7185: AAA166 is the winner (1 Black, 2 White), AAA167 the margin in
/// hundredths, counted in stones (子) under `chineseFlag`.
fn result_text(line: &Fields, chinese: bool) -> String {
    let side = field_int(line, 4);
    let margin = match line.get(&5) {
        Some(Wire::Int(v)) => Some(*v as i32),
        _ => None,
    };
    match margin.filter(|&m| m > 0) {
        Some(m) => {
            let w = f64::from(m) / 100.0;
            let colour = match side {
                1 => "黑",
                2 => "白",
                _ => return "和棋".to_string(),
            };
            if chinese {
                let whole = w as i32;
                let fraction = decimal_to_fraction(w - f64::from(whole));
                if whole != 0 {
                    let extra = if fraction.is_empty() { String::new() } else { format!("又{fraction}") };
                    format!("{colour}胜{whole}{extra}子")
                } else {
                    format!("{colour}胜{fraction}子")
                }
            } else {
                format!("{colour}胜{}目", java_double(w))
            }
        }
        None => match side {
            1 => "黑中盘胜",
            2 => "白中盘胜",
            _ => "和棋",
        }
        .to_string(),
    }
}

/// `decimalToFraction` (OD 2313-2326).
fn decimal_to_fraction(mut e: f64) -> String {
    if e == 0.0 {
        return String::new();
    }
    let mut c = 0;
    while e != e.floor() {
        e *= 10.0;
        c += 1;
    }
    let b = 10f64.powi(c) as i32;
    let nor = e as i32;
    let g = gcd(nor, b);
    format!("{}/{}", nor / g, b / g)
}

fn gcd(a: i32, b: i32) -> i32 {
    if a == 0 { b } else { gcd(b % a, a) }
}

/// Java `String.valueOf(double)` for the margins here: whole numbers keep `.0`.
fn java_double(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e7 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

/// The body of a successful reply, or the failure text: the transport's message or, for a
/// status of 400 or more (`AjaxHttpRequest.observeAjax`'s failure), `HTTP <status>`.
fn reply_body(reply: &HttpReply) -> Result<&str, String> {
    match reply {
        HttpReply::Ok { status, body } if *status < 400 => Ok(body),
        HttpReply::Ok { status, .. } => Err(format!("HTTP {status}")),
        HttpReply::Failed { message, .. } => Err(message.clone()),
    }
}

/// `req0`'s `jQuery[^\(]*\(((?s).*?)\)` matched against the whole reply (OD 1696-1700), read
/// as a JSON object.
fn jsonp_object(body: &str) -> Option<Map<String, Value>> {
    let rest = body.strip_prefix("jQuery")?;
    let open = rest.find('(')?;
    let inner = rest[open + 1..].strip_suffix(')')?;
    match serde_json::from_str(inner) {
        Ok(Value::Object(o)) => Some(o),
        _ => None,
    }
}

/// `getSgf`'s `(jQuery1\(\")([^\"]+)(?s).*`, group 2, URL-decoded (OD 1678, 1119-1125).
fn record_sgf(body: &str) -> Option<String> {
    let rest = body.strip_prefix("jQuery1(\"")?;
    let end = rest.find('"').unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    url_decode(&rest[..end])
}

/// `URLDecoder.decode`: `+` is a space, `%XX` a byte; a malformed escape fails.
fn url_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' => {
                let hex = s.get(i + 1..i + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

/// `JSONObject.optInt`: numbers, and strings that read as numbers; else 0.
fn opt_int(o: &Map<String, Value>, key: &str) -> i64 {
    match o.get(key) {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(0),
        Some(Value::String(s)) => s.trim().parse::<f64>().map_or(0, |f| f as i64),
        _ => 0,
    }
}

/// `JSONObject.optString`: strings as they are, other values as their text; else empty.
fn opt_string(o: &Map<String, Value>, key: &str) -> String {
    match o.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(v) => v.to_string(),
    }
}

fn put_i16(b: &mut Vec<u8>, v: i16) {
    b.extend_from_slice(&v.to_be_bytes());
}

fn put_i32(b: &mut Vec<u8>, v: i32) {
    b.extend_from_slice(&v.to_be_bytes());
}

/// A message `parseReq` cannot read (Java throws and the message is lost).
#[derive(Debug)]
struct Malformed;

/// What `parseReq` acts on.
enum Message {
    /// 23406, the reply to `req1`: the result id, the fragments and whether the rest of the
    /// message read (the follow task starts only then).
    Broadcast { result: i32, frags: Vec<Fragment>, complete: bool },
    /// 23407 (pushed fragments) and 23414 (one transparent fragment).
    Push(Vec<Fragment>),
    /// 23413 replies carry only tips (OD 1971-2005); other ids are ignored.
    Other,
}

/// Bytes `parseReq` skips after the message id: short, int, byte, byte, short, short, short.
const HEAD: usize = 14;

fn parse_message(data: &[u8], fox: bool) -> Result<Message, Malformed> {
    let mut r = Reader { data, pos: 0 };
    r.skip(2)?;
    match r.i16()? {
        MSG_BROADCAST => {
            r.skip(HEAD)?;
            let result = r.i32()?;
            r.skip(16)?;
            let frags = read_fragments(&mut r)?;
            let complete = read_trailer(&mut r, fox).is_ok();
            Ok(Message::Broadcast { result, frags, complete })
        }
        MSG_PUSH => {
            r.skip(HEAD + 16)?;
            Ok(Message::Push(read_fragments(&mut r)?))
        }
        MSG_TRANSPARENT => {
            r.skip(HEAD + 20)?;
            let len = r.i16()?;
            if len > 0 {
                Ok(Message::Push(vec![Fragment::parse(r.take(len as usize)?)?]))
            } else {
                Ok(Message::Other)
            }
        }
        _ => Ok(Message::Other),
    }
}

/// `showFragmentNum`, then each fragment as a short length and its bytes (OD 1878-1886).
fn read_fragments(r: &mut Reader) -> Result<Vec<Fragment>, Malformed> {
    let count = r.i32()?;
    let mut frags = Vec::new();
    for _ in 0..count.max(0) {
        let len = usize::try_from(r.i16()?).map_err(|_| Malformed)?;
        frags.push(Fragment::parse(r.take(len)?)?);
    }
    Ok(frags)
}

/// The end of a 23406 reply (OD 1896-1910).
fn read_trailer(r: &mut Reader, fox: bool) -> Result<(), Malformed> {
    r.skip(12)?;
    if r.i16()? <= 0 {
        r.skip(1)?;
    }
    if fox {
        r.skip(12)?;
    }
    Ok(())
}

/// Big-endian reads that fail where Java's `ByteBuffer` underflows.
struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Malformed> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len()).ok_or(Malformed)?;
        let bytes = &self.data[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }

    fn skip(&mut self, n: usize) -> Result<(), Malformed> {
        self.take(n).map(|_| ())
    }

    fn u8(&mut self) -> Result<u8, Malformed> {
        Ok(self.take(1)?[0])
    }

    fn i16(&mut self) -> Result<i16, Malformed> {
        let b = self.take(2)?;
        Ok(i16::from_be_bytes([b[0], b[1]]))
    }

    fn i32(&mut self) -> Result<i32, Malformed> {
        let b = self.take(4)?;
        Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn at_end(&self) -> bool {
        self.pos >= self.data.len()
    }

    /// `uint32` (OD 2801-2821): at most five bytes, a sixth skipped.
    fn uint32(&mut self) -> Result<u64, Malformed> {
        let mut value = 0u64;
        for shift in [0, 7, 14, 21] {
            let b = u64::from(self.u8()?);
            value |= (b & 127) << shift;
            if b < 128 {
                return Ok(value);
            }
        }
        let b = u64::from(self.u8()?);
        value |= (b & 15) << 28;
        if b >= 128 {
            self.u8()?;
        }
        Ok(value)
    }

    /// `uint64` (OD 2823-2877): a varint of up to ten bytes.
    fn uint64(&mut self) -> Result<u64, Malformed> {
        let mut value = 0u64;
        for k in 0..10 {
            let b = u64::from(self.u8()?);
            value |= (b & 127) << (7 * k);
            if b < 128 {
                break;
            }
        }
        Ok(value)
    }

    /// `readString` (OD 2884-2939): a length-prefixed UTF-8 string; a length past the end
    /// reads as empty and consumes only the length.
    fn string(&mut self) -> Result<String, Malformed> {
        let len = self.uint32()?;
        let fits = usize::try_from(len)
            .ok()
            .and_then(|n| self.pos.checked_add(n))
            .is_some_and(|end| end <= self.data.len());
        if !fits {
            return Ok(String::new());
        }
        Ok(String::from_utf8_lossy(self.take(len as usize)?).into_owned())
    }
}

/// A decoded field (`JSONObject` value).
enum Wire {
    Int(u64),
    Text(String),
    Msg(Fields),
    Ints(Vec<u64>),
}

/// Decoded fields by protobuf field number (Java names them `AAA…`).
type Fields = BTreeMap<u64, Wire>;

/// `optInt` of a decoded field: the low 32 bits as an int; 0 when absent.
fn field_int(fields: &Fields, field: u64) -> i64 {
    match fields.get(&field) {
        Some(Wire::Int(v)) => i64::from(*v as i32),
        _ => 0,
    }
}

/// `optString` of a decoded string field.
fn field_text(fields: &Fields, field: u64) -> String {
    match fields.get(&field) {
        Some(Wire::Text(s)) => s.clone(),
        _ => String::new(),
    }
}

#[derive(Clone, Copy)]
enum Kind {
    U32,
    U64,
    Str,
    Msg(&'static Spec),
    RepeatedU32,
    RepeatedStr,
    /// Read nothing (`decode17` field 7, `// TODO AAA80`).
    Nothing,
}

/// A Java `decodeNN` method: its fields and whether an unknown field ends the message
/// (`break`/`return m`) or is passed over without reading its value (`decode1`, `decode48`).
struct Spec {
    fields: &'static [(u64, Kind)],
    stop_on_unknown: bool,
}

/// `decode1` (OD 2458-2521), AAA307: 4 handicap, 5 komi, 10 rule, 16 board size.
const GAME_INFO: Spec = Spec {
    fields: &[
        (1, Kind::U64),
        (2, Kind::U64),
        (3, Kind::U32),
        (4, Kind::U32),
        (5, Kind::U32),
        (6, Kind::U32),
        (7, Kind::U32),
        (8, Kind::U32),
        (9, Kind::U32),
        (10, Kind::U32),
        (11, Kind::U32),
        (12, Kind::U32),
        (13, Kind::U32),
        (14, Kind::U32),
        (15, Kind::U32),
        (16, Kind::U32),
        (17, Kind::U32),
    ],
    stop_on_unknown: false,
};

/// `decode48` (OD 2523-2565), a player: 1 AAA224 name, 2 AAA225 Chinese name, 4 AAA227 region.
const PLAYER: Spec = Spec {
    fields: &[
        (1, Kind::Str),
        (2, Kind::Str),
        (3, Kind::U32),
        (4, Kind::U32),
        (5, Kind::U32),
        (6, Kind::U32),
        (7, Kind::U32),
        (8, Kind::U32),
        (9, Kind::U64),
        (10, Kind::Str),
    ],
    stop_on_unknown: false,
};

/// `decode52` (OD 2417-2456), 20032: 6 AAA307 game, 7 AAA308 White, 8 AAA309 Black.
const NEW_GAME: Spec = Spec {
    fields: &[
        (1, Kind::U32),
        (2, Kind::U64),
        (3, Kind::U32),
        (4, Kind::U64),
        (5, Kind::U64),
        (6, Kind::Msg(&GAME_INFO)),
        (7, Kind::Msg(&PLAYER)),
        (8, Kind::Msg(&PLAYER)),
        (9, Kind::U64),
    ],
    stop_on_unknown: true,
};

/// `decode53` (OD 2639-2658), 4116.
const SWAP: Spec = Spec {
    fields: &[(1, Kind::U32), (2, Kind::U32)],
    stop_on_unknown: true,
};

/// `decode20` (OD 2567-2604), 7005: 4 AAA106 point, 6 AAA158 colour, 8 AAA102 move number.
const MOVE: Spec = Spec {
    fields: &[
        (1, Kind::U32),
        (2, Kind::U64),
        (3, Kind::U32),
        (4, Kind::U32),
        (5, Kind::U32),
        (6, Kind::U32),
        (7, Kind::U32),
        (8, Kind::U32),
    ],
    stop_on_unknown: true,
};

/// `decode35` (OD 2606-2637), 7185: 4 AAA166 winner, 5 AAA167 margin.
const RESULT: Spec = Spec {
    fields: &[
        (1, Kind::U32),
        (2, Kind::U64),
        (3, Kind::U32),
        (4, Kind::U32),
        (5, Kind::U32),
        (6, Kind::U32),
    ],
    stop_on_unknown: true,
};

/// `decode7` (OD 2660-2691), 8005: 4 AAA72 move number, 5 AAA37 comment.
const COMMENT: Spec = Spec {
    fields: &[
        (1, Kind::U32),
        (2, Kind::U64),
        (3, Kind::U32),
        (4, Kind::U32),
        (5, Kind::Str),
        (6, Kind::U32),
    ],
    stop_on_unknown: true,
};

/// `decode2` (OD 2732-2766), AAA79: 1 AAA106 moves, 3 AAA20 move number, 4 AAA283 comment.
const VARIATION: Spec = Spec {
    fields: &[
        (1, Kind::RepeatedU32),
        (2, Kind::U32),
        (3, Kind::U32),
        (4, Kind::Str),
        (5, Kind::RepeatedStr),
    ],
    stop_on_unknown: true,
};

/// `decode17` (OD 2693-2730), 8185: 6 AAA79 variation.
const BRANCH: Spec = Spec {
    fields: &[
        (1, Kind::U32),
        (2, Kind::U64),
        (3, Kind::U32),
        (4, Kind::U32),
        (5, Kind::U32),
        (6, Kind::Msg(&VARIATION)),
        (7, Kind::Nothing),
    ],
    stop_on_unknown: true,
};

/// `decode42` (OD 2768-2799), 7198 clock times.
const CLOCK: Spec = Spec {
    fields: &[
        (1, Kind::U32),
        (2, Kind::U64),
        (3, Kind::U32),
        (4, Kind::U32),
        (5, Kind::U32),
        (6, Kind::U32),
    ],
    stop_on_unknown: true,
};

fn decode(spec: &Spec, bytes: &[u8]) -> Result<Fields, Malformed> {
    let mut r = Reader { data: bytes, pos: 0 };
    let mut m = Fields::new();
    while !r.at_end() {
        let field = r.uint32()? >> 3;
        let Some(&(_, kind)) = spec.fields.iter().find(|(f, _)| *f == field) else {
            if spec.stop_on_unknown {
                break;
            }
            continue;
        };
        match kind {
            Kind::U32 => {
                m.insert(field, Wire::Int(r.uint32()?));
            }
            Kind::U64 => {
                m.insert(field, Wire::Int(r.uint64()?));
            }
            Kind::Str => {
                m.insert(field, Wire::Text(r.string()?));
            }
            Kind::Msg(sub) => {
                let len = usize::try_from(r.uint32()?).map_err(|_| Malformed)?;
                m.insert(field, Wire::Msg(decode(sub, r.take(len)?)?));
            }
            Kind::RepeatedU32 => {
                let v = r.uint32()?;
                match m.entry(field).or_insert_with(|| Wire::Ints(Vec::new())) {
                    Wire::Ints(list) => list.push(v),
                    other => *other = Wire::Ints(vec![v]),
                }
            }
            // AAA37 (commentary lines of a variation) is never used (OD 2753-2758, 2248-2282).
            Kind::RepeatedStr => {
                r.string()?;
            }
            Kind::Nothing => {}
        }
    }
    Ok(m)
}

/// Java `Fragment` (OD 2388-2415): the type and its decoded fields.
struct Fragment {
    kind: u32,
    line: Fields,
}

impl Fragment {
    fn parse(bytes: &[u8]) -> Result<Fragment, Malformed> {
        let (kind, raw) = parse_proto(bytes).ok_or(Malformed)?;
        let spec = match kind {
            FRAG_NEW_GAME => Some(&NEW_GAME),
            FRAG_SWAP_PLAYERS => Some(&SWAP),
            FRAG_MOVE => Some(&MOVE),
            FRAG_COMMENT => Some(&COMMENT),
            // 7025 is dropped (`o.type = 0`, OD 2405-2407).
            7025 => return Ok(Fragment { kind: 0, line: Fields::new() }),
            FRAG_BRANCH => Some(&BRANCH),
            FRAG_RESULT => Some(&RESULT),
            7198 => Some(&CLOCK),
            _ => None,
        };
        let line = match spec {
            Some(spec) => decode(spec, raw)?,
            None => Fields::new(),
        };
        Ok(Fragment { kind, line })
    }
}

/// `parseProto` (OD 2941-2970): a big-endian u16 size (32768: a u32 size follows), a u16 type,
/// one skipped byte, the body, and four trailing bytes.
fn parse_proto(e: &[u8]) -> Option<(u32, &[u8])> {
    let len = e.len();
    if len <= 2 {
        return None;
    }
    let first = u32::from(u16::from_be_bytes([e[0], e[1]]));
    let mut r = 2;
    let mut rest = len - 2;
    let size = if first == 32768 {
        if rest <= 4 {
            return None;
        }
        r = 6;
        rest -= 4;
        u32::from_be_bytes([e[2], e[3], e[4], e[5]])
    } else {
        first
    };
    if !(7..0x8000_0000).contains(&size) || (rest as u64) < u64::from(size) {
        return None;
    }
    let kind = u32::from(u16::from_be_bytes([e[r], e[r + 1]]));
    r += 2;
    Some((kind, &e[r + 1..len - 4]))
}

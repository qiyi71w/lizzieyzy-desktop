//! 弈客 live / game rooms over Socket.IO (`OnlineDialog.req2`, types 1 and 5):
//! `rtgame.yikeweiqi.com`, Engine.IO v4 over WebSocket.
//!
//! Java (fixed `cb53acc`, OD = `gui/OnlineDialog.java`) follows the room with io.socket
//! `socket.io-client` 2.1.2 (`pom.xml:300-303`, Engine.IO client 2.1.0) and its defaults. The
//! parts of that client this session reproduces, with their sources at those tags:
//! - Engine.IO: the handshake `0{..}` opens the connection; every packet received re-arms the
//!   heartbeat timer of `pingInterval + pingTimeout`, whose expiry closes the connection
//!   ("ping timeout"); a ping `2` is answered with a pong `3` (engine.io-client
//!   `Socket.onPacket`/`onHandshake`/`onHeartbeat`).
//! - Socket.IO: after the handshake the client connects the default namespace with `40`; the
//!   server's `40{"sid"}` connects it. Events received before that are held and delivered first.
//!   Acknowledged emits carry ids counted from 0 per socket; pending acks are dropped when the
//!   connection closes; `41` (server disconnect) and `44` (connect error) end the socket for good
//!   (`Socket.emit`/`onpacket`/`onconnect`/`emitBuffered`/`ondisconnect`/`onclose`).
//! - Reconnection (`Manager` defaults: `reconnection` on, unlimited attempts,
//!   `reconnectionDelay` 1000 ms, `reconnectionDelayMax` 5000 ms, `randomizationFactor` 0.5,
//!   `timeout` 20000 ms): a connection that closes after it opened reconnects with a reset
//!   backoff; a connection attempt that fails or does not open within the timeout reconnects
//!   with the next backoff step (`Manager.onclose`/`reconnect`/`open`, `backo.Backoff.duration`).
//!   An explicit close (`sio.close()` / `sio.disconnect()`) never reconnects.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, Hasher};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::yike::mainline_only;
use super::{branch_actions, Connect, Frame, OnlineText, SourceUpdate, Step};
use crate::formats;
use crate::record::{BoardSize, DocumentEdit, GameRecord, NodeKind, PlayRules, Point, RecordFormat, Stone, ROOT};

/// The WebSocket address of the Socket.IO server (OD 153-156 `c1`, `https://rtgame.yikeweiqi.com`,
/// with the client's default path `/socket.io`).
pub fn socket_url() -> String {
    "wss://rtgame.yikeweiqi.com/socket.io/?EIO=4&transport=websocket".into()
}

/// `Manager` `reconnectionDelay` / `reconnectionDelayMax` / `randomizationFactor` defaults.
const RECONNECT_MIN_MS: u64 = 1000;
const RECONNECT_MAX_MS: u64 = 5000;
const RANDOMIZATION: f64 = 0.5;
/// `Manager.Options.timeout`: an attempt that has not opened by then fails.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// OD 122 `userId` before the server's `userinfo`.
const INITIAL_USER_ID: i64 = -1_000_000;

/// The state of the current connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Link {
    /// A connection was asked for and has not opened its WebSocket yet.
    Waiting,
    /// The WebSocket is open; the Engine.IO handshake has not arrived.
    Opening,
    /// The Engine.IO handshake arrived.
    Open,
}

/// The emit an acknowledgement answers (OD 3254-3395).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ack {
    Login,
    EntryRoom,
    Channel,
    Listen,
}

/// The room's game as the session holds it (OD `history`, `blackPlayer`/`whitePlayer`).
struct Game {
    record: GameRecord,
    names: (String, String),
    komi: Option<f64>,
    handicap: Option<u32>,
}

impl Game {
    fn update(&self) -> SourceUpdate {
        SourceUpdate::Game {
            record: Box::new(self.record.clone()),
            names: Some(self.names.clone()),
            komi: self.komi,
            handicap: self.handicap,
        }
    }
}

/// One followed room.
pub struct Session {
    room: u64,
    user_id: i64,
    link: Link,
    /// The Socket.IO namespace is connected (`Socket.connected`).
    connected: bool,
    /// The connect timeout while not open, the heartbeat deadline once open.
    due: Option<Instant>,
    /// `pingInterval + pingTimeout` of the handshake.
    heartbeat: Duration,
    /// `Backoff.attempts`.
    attempts: u32,
    /// `Socket.ids`.
    next_ack: u64,
    acks: Vec<(u64, Ack)>,
    /// Events received before the namespace connected (`Socket.receiveBuffer`).
    held: Vec<Vec<Value>>,
    /// The socket was closed on purpose; nothing reconnects it.
    ended: bool,
    game: Option<Game>,
    /// Commentator branches already reported, by `handsCount` and `id` (OD 132-133 `branchs`).
    branches: HashSet<(i64, i64)>,
    /// The last text reported for each comment (move, id): a reconnect's `comment/listen` replay
    /// adds nothing again, and an update replaces only that comment's line.
    comments: HashMap<(i64, i64), String>,
}

impl Session {
    /// A session for `room`; the returned step opens the first connection (OD 3039-3252 `req2`).
    pub fn new(room: u64) -> (Self, Step) {
        let session = Session {
            room,
            user_id: INITIAL_USER_ID,
            link: Link::Waiting,
            connected: false,
            due: Some(Instant::now() + CONNECT_TIMEOUT),
            heartbeat: Duration::ZERO,
            attempts: 0,
            next_ack: 0,
            acks: Vec::new(),
            held: Vec::new(),
            ended: false,
            game: None,
            branches: HashSet::new(),
            comments: HashMap::new(),
        };
        let step = Step {
            connect: Some(Connect { url: socket_url(), after: Duration::ZERO }),
            ..Step::default()
        };
        (session, step)
    }

    /// The WebSocket opened; the server sends the Engine.IO handshake next.
    pub fn on_open(&mut self) -> Step {
        if !self.ended && self.link == Link::Waiting {
            self.link = Link::Opening;
        }
        Step::default()
    }

    /// One Engine.IO packet.
    pub fn on_text(&mut self, text: &str) -> Step {
        let mut step = Step::default();
        if self.ended {
            return step;
        }
        let now = Instant::now();
        if self.link == Link::Open {
            self.due = Some(now + self.heartbeat);
        }
        let Some(kind) = text.chars().next() else { return step };
        let body = &text[kind.len_utf8()..];
        match kind {
            '0' => {
                let Some(heartbeat) = handshake(body) else {
                    // A handshake that does not parse is a failed attempt.
                    return self.reconnect(now);
                };
                self.link = Link::Open;
                self.attempts = 0;
                self.heartbeat = heartbeat;
                self.due = Some(now + heartbeat);
                step.send.push(Frame::Text("40".into()));
            }
            '1' => return self.reconnect(now),
            '2' => step.send.push(Frame::Text("3".into())),
            '4' => self.on_socket_packet(body, &mut step),
            _ => {}
        }
        step
    }

    /// The connection ended: one that had opened reconnects after a reset backoff, a failed
    /// attempt after the next backoff step. The error text is not used (Java only logs it).
    pub fn on_closed(&mut self, error: Option<&str>) -> Step {
        let _ = error;
        if self.ended {
            return Step::default();
        }
        self.reconnect(Instant::now())
    }

    /// The connect timeout or the heartbeat expired: the connection counts as closed.
    pub fn tick(&mut self, now: Instant) -> Step {
        match self.due {
            Some(due) if !self.ended && now >= due => self.reconnect(now),
            _ => Step::default(),
        }
    }

    /// When [`Session::tick`] has work: the connect timeout of an attempt that has not opened,
    /// else the heartbeat deadline; none once the socket was closed for good.
    pub fn next_due(&self) -> Option<Instant> {
        if self.ended {
            None
        } else {
            self.due
        }
    }

    /// Closes the current connection and asks for the next one (`Manager.onclose`/`reconnect`).
    fn reconnect(&mut self, now: Instant) -> Step {
        if self.link == Link::Open {
            self.attempts = 0;
        }
        let after = self.backoff();
        self.drop_connection();
        self.due = Some(now + after + CONNECT_TIMEOUT);
        Step {
            close: true,
            connect: Some(Connect { url: socket_url(), after }),
            ..Step::default()
        }
    }

    /// `Backoff.duration`: `1000 · 2^attempts`, moved by up to half of it in a random
    /// direction, within 1000..=5000 ms.
    fn backoff(&mut self) -> Duration {
        let ms = RECONNECT_MIN_MS << self.attempts.min(32);
        self.attempts = self.attempts.saturating_add(1);
        let rand = random_unit();
        let deviation = (rand * RANDOMIZATION * ms as f64) as u64;
        let ms = if ((rand * 10.0) as u64) & 1 == 0 {
            ms.saturating_sub(deviation)
        } else {
            ms.saturating_add(deviation)
        };
        Duration::from_millis(ms.clamp(RECONNECT_MIN_MS, RECONNECT_MAX_MS))
    }

    fn drop_connection(&mut self) {
        self.link = Link::Waiting;
        self.connected = false;
        self.acks.clear();
        self.held.clear();
    }

    /// `sio.close()` / `sio.disconnect()`: the socket closes and never reconnects.
    fn end(&mut self, step: &mut Step) {
        self.drop_connection();
        self.ended = true;
        self.due = None;
        step.close = true;
    }

    /// A Socket.IO packet of the default namespace (`IOParser.Decoder`, `Socket.onpacket`).
    fn on_socket_packet(&mut self, body: &str, step: &mut Step) {
        let Some(kind) = body.chars().next() else { return };
        let mut rest = &body[kind.len_utf8()..];
        if rest.starts_with('/') {
            let (nsp, after) = rest.split_once(',').unwrap_or((rest, ""));
            if nsp != "/" {
                return;
            }
            rest = after;
        }
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        let id = rest[..digits].parse::<u64>().ok();
        let payload = &rest[digits..];
        match kind {
            '0' => {
                let sid = serde_json::from_str::<Value>(payload)
                    .is_ok_and(|v| v.get("sid").is_some_and(Value::is_string));
                if sid {
                    self.connected = true;
                    for args in std::mem::take(&mut self.held) {
                        // An event that restarted or ended the follow drops the rest.
                        if !self.connected {
                            return;
                        }
                        self.on_event(args, step);
                    }
                    if self.connected {
                        self.login(step);
                    }
                }
            }
            '1' | '4' => self.end(step),
            '2' => {
                let Ok(args) = serde_json::from_str::<Vec<Value>>(payload) else { return };
                if self.connected {
                    self.on_event(args, step);
                } else {
                    self.held.push(args);
                }
            }
            '3' => {
                let (Some(id), Ok(args)) = (id, serde_json::from_str::<Vec<Value>>(payload)) else { return };
                self.on_ack(id, &args, step);
            }
            _ => {}
        }
    }

    /// `sendData` (OD 4418-4458): an emit whose ack goes through [`Session::on_ack`].
    fn emit(&mut self, step: &mut Step, event: &str, data: Value, ack: Ack) {
        let id = self.next_ack;
        self.next_ack += 1;
        self.acks.push((id, ack));
        let packet = Value::Array(vec![event.into(), data]);
        step.send.push(Frame::Text(format!("42{id}{packet}")));
    }

    /// `login` (OD 3254-3270): its ack does nothing; the server answers with `userinfo`.
    fn login(&mut self, step: &mut Step) {
        let data = json!({ "hall": "1", "room": self.room, "token": -1, "user_id": self.user_id, "platform": 3 });
        self.emit(step, "login", data, Ack::Login);
    }

    /// `entry` (OD 3272-3287).
    fn entry(&mut self, step: &mut Step) {
        let data = json!({ "hall": "1", "room": self.room, "platform": 3, "user_id": self.user_id });
        self.emit(step, "entry_room", data, Ack::EntryRoom);
    }

    /// `channel` (OD 3360-3375).
    fn channel(&mut self, step: &mut Step) {
        let channel = format!("chat_1_{}", self.room);
        let data = json!({ "hall": "1", "room": self.room, "platform": 3, "channel": channel });
        self.emit(step, "channel/add", data, Ack::Channel);
    }

    /// `listen` (OD 3377-3395).
    fn listen(&mut self, step: &mut Step) {
        let data = json!({ "hall": "1", "room": self.room, "platform": 3 });
        self.emit(step, "comment/listen", data, Ack::Listen);
    }

    /// The ack envelope of `sendData` (OD 4423-4456): code 0 passes `data` on, codes 1 and 2
    /// pass `message` on, code 10 disconnects. A first argument that is not an object, a missing
    /// code, or a `data` / `message` that is neither object nor array throws in Java and ends the
    /// chain there. Codes 1 and 2 also show the message as a sync failure.
    fn on_ack(&mut self, id: u64, args: &[Value], step: &mut Step) {
        let Some(at) = self.acks.iter().position(|&(i, _)| i == id) else { return };
        let (_, ack) = self.acks.remove(at);
        let data = match args.first() {
            None => None,
            Some(Value::Object(envelope)) => {
                let Some(code) = envelope.get("code").and_then(Value::as_i64) else { return };
                let passed = match code {
                    0 => envelope.get("data"),
                    1 | 2 => {
                        let message = envelope.get("message");
                        let args = match message {
                            None | Some(Value::Null) => Vec::new(),
                            Some(Value::String(s)) => vec![s.clone()],
                            Some(other) => vec![other.to_string()],
                        };
                        let text = OnlineText::with("YikeLiveDialog.syncFailed", args);
                        step.updates.push(SourceUpdate::Status(text));
                        message
                    }
                    10 => {
                        self.end(step);
                        step.updates.push(SourceUpdate::Stopped(OnlineText::new("Online.status.stopped")));
                        return;
                    }
                    _ => None,
                };
                match passed {
                    None | Some(Value::Null) => None,
                    Some(v @ (Value::Object(_) | Value::Array(_))) => Some(v.clone()),
                    Some(_) => return,
                }
            }
            Some(_) => return,
        };
        match ack {
            Ack::Login => {}
            Ack::EntryRoom => self.channel(step),
            Ack::Channel => self.listen(step),
            Ack::Listen => self.listen_back(data, step),
        }
    }

    /// The room's events (OD 3173-3242).
    fn on_event(&mut self, args: Vec<Value>, step: &mut Step) {
        let Some(name) = args.first().and_then(Value::as_str) else { return };
        let arg = args.get(1).and_then(Value::as_object);
        match name {
            "userinfo" => {
                // OD 3173-3189: `user_id` of the first argument (0 when absent), then `entry`.
                if args.len() > 1 {
                    let Some(info) = arg else { return };
                    self.user_id = int(info, "user_id").unwrap_or(0);
                }
                self.entry(step);
            }
            "init" => {
                if let Some(data) = arg {
                    self.init_data(data, step);
                }
            }
            "move" => {
                if let Some(update) = arg.and_then(|d| self.on_move(d)) {
                    step.updates.push(update);
                }
            }
            "update_game" => {
                if let Some(g) = arg {
                    self.update_game(g, step);
                }
            }
            "move_delete" => self.restart(step),
            "comments" => {
                if let Some(update) = arg.and_then(|cb| self.proc_comments(cb)) {
                    step.updates.push(update);
                }
            }
            _ => {}
        }
    }

    /// `initData` (OD 3289-3358): the game from the main line of `game_info.sgf` on a board of
    /// `game_info.boardSize` (default 19), the players, komi and handicap beside it, and the
    /// result of a finished game (status 3). Without a readable SGF Java leaves the board alone,
    /// closes the socket and, for a game with a result, reads the room once from the old API
    /// (`refresh`): reported as [`SourceUpdate::NoGame`].
    ///
    /// Java reopens the board here, which stops its own sync when the size changes
    /// (`Board.reopen` → `clear` → `stopSync`); the session only reports the game.
    fn init_data(&mut self, data: &Map<String, Value>, step: &mut Step) {
        let Some(info) = data.get("game_info").and_then(Value::as_object) else { return };
        let size = int(info, "boardSize")
            .and_then(|n| u16::try_from(n).ok())
            .filter(|n| (2..=52).contains(n))
            .unwrap_or(19);
        let size = BoardSize { width: size, height: size };
        let names = (string(info, "blackName"), string(info, "whiteName"));
        let sgf = mainline_only(&string(info, "sgf"));
        let Some(record) = read_record(&sgf, size) else {
            self.game = None;
            let finished = !blank(&string(info, "resultDesc"));
            step.updates.push(SourceUpdate::NoGame { finished });
            self.end(step);
            return;
        };
        let game = Game {
            record,
            names,
            komi: float(info, "komi"),
            handicap: int(info, "handicap").and_then(|n| u32::try_from(n).ok()),
        };
        step.updates.push(game.update());
        self.game = Some(game);
        if string(info, "status") == "3" {
            let result = string(info, "resultDesc");
            if !blank(&result) {
                step.updates.push(SourceUpdate::Finished { result, set_result: true });
            }
        }
    }

    /// `move` (OD 4317-4362): from the end of the main line, moves from `mcnt` on are taken
    /// back (a node kept when it holds the same point as the new move at `mcnt`, or when it has
    /// other variations), then the stone (or a pass off the board) is played.
    fn on_move(&mut self, d: &Map<String, Value>) -> Option<SourceUpdate> {
        let game = self.game.as_mut()?;
        let m = d.get("move")?.as_object()?;
        let number = int(m, "mcnt").unwrap_or(0);
        if number <= 0 {
            return None;
        }
        let rec = &mut game.record;
        let (x, y) = (int(m, "x").unwrap_or(0), int(m, "y").unwrap_or(0));
        let point = match (u16::try_from(x), u16::try_from(y)) {
            (Ok(x), Ok(y)) if rec.size.contains(Point { x, y }) => Some(Point { x, y }),
            _ => None,
        };
        let color = if int(m, "c") == Some(-1) { Stone::White } else { Stone::Black };
        let mut node = rec.line_end(ROOT);
        let current = i64::from(rec.node(node).move_number);
        let mut changed = false;
        let mut change_move = false;
        if number <= current {
            for i in number..=current {
                let same = i == current
                    && matches!((rec.node(node).kind, point), (NodeKind::Move { point: p, .. }, Some(q)) if p == q);
                let Some(parent) = rec.parent(node) else { continue };
                if rec.node(parent).children.len() <= 1 && !same {
                    changed |= rec.apply(&DocumentEdit::Remove { node }).is_ok_and(|a| a.changed);
                    change_move = false;
                } else {
                    change_move = true;
                }
                node = parent;
            }
        }
        let edit = match point {
            Some(point) => DocumentEdit::Play { at: node, color, point, alternate: !change_move, rules: PlayRules::default() },
            None => DocumentEdit::Pass { at: node, color },
        };
        changed |= rec.apply(&edit).is_ok_and(|a| a.changed);
        changed.then(|| game.update())
    }

    /// `updateGame` (OD 4399-4416): status 3 closes the socket and reports the result.
    fn update_game(&mut self, g: &Map<String, Value>, step: &mut Step) {
        if int(g, "status") != Some(3) {
            return;
        }
        self.end(step);
        let result = string(g, "resultDesc");
        if !blank(&result) {
            step.updates.push(SourceUpdate::Finished { result, set_result: true });
        }
    }

    /// `moveDelete` (OD 4364-4397): the follow starts again on a new socket (`proc` → `req2`,
    /// OD 855-894): the game is dropped until the next `init`, and the new socket connects at
    /// once with fresh ack ids and backoff. The user id and the reported branches stay.
    fn restart(&mut self, step: &mut Step) {
        self.drop_connection();
        self.game = None;
        self.next_ack = 0;
        self.attempts = 0;
        self.due = Some(Instant::now() + CONNECT_TIMEOUT);
        step.close = true;
        step.connect = Some(Connect { url: socket_url(), after: Duration::ZERO });
    }

    /// `procComments` (OD 4230-4245): an added or updated commentator branch (type 1) or
    /// comment (type 2).
    fn proc_comments(&mut self, cb: &Map<String, Value>) -> Option<SourceUpdate> {
        let kind = string(cb, "type");
        if kind != "add" && kind != "update" {
            return None;
        }
        let d = cb.get("content")?.as_object()?;
        match int(d, "type") {
            Some(1) => self.add_branch(d),
            Some(2) => self.add_comment(d),
            _ => None,
        }
    }

    /// `listenBack` (OD 3397-3414): the room's branches and comments so far; Java stops at an
    /// entry that is not an object.
    fn listen_back(&mut self, data: Option<Value>, step: &mut Step) {
        let Some(Value::Array(items)) = data else { return };
        for item in &items {
            let Some(d) = item.as_object() else { return };
            let update = match int(d, "type") {
                Some(1) => self.add_branch(d),
                Some(2) => self.add_comment(d),
                _ => None,
            };
            step.updates.extend(update);
        }
    }

    /// `addBranch` (OD 4247-4282): each branch `id` once per move; a non-blank `content` after
    /// a move greater than 0 is SGF node text read from that move (`SGFParser.parseBranch`).
    fn add_branch(&mut self, d: &Map<String, Value>) -> Option<SourceUpdate> {
        let number = int(d, "handsCount").unwrap_or(0);
        let id = int(d, "id").unwrap_or(0);
        if !self.branches.insert((number, id)) {
            return None;
        }
        let content = string(d, "content");
        if blank(&content) || number <= 0 {
            return None;
        }
        // `history.goToMoveNumber(move)` on the room's own game.
        let record = &self.game.as_ref()?.record;
        let mut at = ROOT;
        while u64::from(record.node(at).move_number) < number as u64 {
            at = record.main_child(at)?;
        }
        if i64::from(record.node(at).move_number) != number {
            return None;
        }
        let actions = branch_actions(&content, record, at);
        if actions.is_empty() {
            return None;
        }
        Some(SourceUpdate::Branch { move_number: u32::try_from(number).unwrap_or(u32::MAX), actions })
    }

    /// `addComment` (OD 4284-4315): `LiveMember：content` (the member from the `extend` JSON
    /// text), or the content alone without a member; a blank text adds nothing, and an `extend`
    /// that is not a JSON object throws in Java. A new text for a comment already reported
    /// replaces that comment's line; Java's `update` overwrote the node's whole comment, other
    /// commentators' lines and the user's text included.
    fn add_comment(&mut self, d: &Map<String, Value>) -> Option<SourceUpdate> {
        let Ok(Value::Object(extend)) = serde_json::from_str::<Value>(&string(d, "extend")) else { return None };
        let member = string(&extend, "LiveMember");
        let content = string(d, "content");
        let text = if blank(&member) { content } else { format!("{member}：{content}") };
        if blank(&text) {
            return None;
        }
        let number = int(d, "handsCount").unwrap_or(0);
        let id = int(d, "id").unwrap_or(0);
        let move_number = u32::try_from(number.max(0)).unwrap_or(u32::MAX);
        match self.comments.insert((number, id), text.clone()) {
            Some(old) if old == text => None,
            Some(old) => Some(SourceUpdate::CommentUpdate { move_number, old, text }),
            None => Some(SourceUpdate::Comment { move_number, text }),
        }
    }
}

/// `pingInterval + pingTimeout` of an Engine.IO handshake (`HandshakeData`).
fn handshake(body: &str) -> Option<Duration> {
    let data: Value = serde_json::from_str(body).ok()?;
    let interval = data.get("pingInterval")?.as_u64()?;
    let timeout = data.get("pingTimeout")?.as_u64()?;
    Some(Duration::from_millis(interval.saturating_add(timeout)))
}

/// The SGF read as a record of `size` (Java syncs the parsed game onto the board reopened at
/// `boardSize`): an SGF of another size is read again with `SZ` of that size first.
fn read_record(sgf: &str, size: BoardSize) -> Option<GameRecord> {
    let record = formats::parse(RecordFormat::Sgf, sgf, false).ok()?;
    if record.size == size {
        return Some(record);
    }
    let Some(at) = sgf.find('(').and_then(|open| sgf[open..].find(';').map(|semi| open + semi + 1)) else {
        return Some(record);
    };
    let sized = format!("{}SZ[{}]{}", &sgf[..at], size.width, &sgf[at..]);
    formats::parse(RecordFormat::Sgf, &sized, false).ok().or(Some(record))
}

/// `Utils.isBlank`: empty after Java's `trim` (characters up to U+0020).
fn blank(s: &str) -> bool {
    s.trim_matches(|c: char| c <= ' ').is_empty()
}

/// `JSONObject.optString`: a string as is, another value as its JSON text, `""` when absent.
fn string(o: &Map<String, Value>, key: &str) -> String {
    match o.get(key) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
    }
}

/// `JSONObject.optInt` / `optLong` without the default: a number (truncated) or a numeric string.
fn int(o: &Map<String, Value>, key: &str) -> Option<i64> {
    match o.get(key)? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => {
            let s = s.trim();
            s.parse::<i64>().ok().or_else(|| s.parse::<f64>().ok().filter(|f| f.is_finite()).map(|f| f as i64))
        }
        _ => None,
    }
}

/// `JSONObject.optDouble` without the default.
fn float(o: &Map<String, Value>, key: &str) -> Option<f64> {
    match o.get(key)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok().filter(|f| f.is_finite()),
        _ => None,
    }
}

/// A random number in `[0, 1)` (`Math.random()` of the backoff jitter), from the standard
/// library's randomly keyed hasher.
fn random_unit() -> f64 {
    let bits = std::collections::hash_map::RandomState::new().build_hasher().finish();
    (bits >> 11) as f64 / (1u64 << 53) as f64
}

//! KaTrain-compatible JSON analysis queries behind the ordinary GTP engine interface.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use fancy_regex::Regex;
use serde_json::{json, Value};

use super::{EngineInput, Output};
use crate::engine::rules::KataRules;

const COMMANDS: &str = "protocol_version\nname\nversion\nlist_commands\nboardsize\nrectangular_boardsize\nkomi\nkata-get-rules\nkata-get-param\nkata-set-param\nkata-set-rules\nclear_board\nset_position\nloadsgf\nplay\nundo\nkata-analyze\nkata-genmove_analyze\ngenmove\nkata-list_time_settings\nkata-time_settings\nstop\nquit";
const MAX_VISITS: f64 = (1u64 << 50) as f64;

pub(super) enum Action {
    None,
    /// Payloads to write in order: a superseded query's `terminate`, then any new query.
    Send(Vec<String>),
    Quit,
}

#[derive(Clone)]
struct Move { color: &'static str, point: String }

struct Query {
    sequence: u64,
    id: String,
    response_id: String,
    genmove: bool,
    /// `kata-genmove_analyze`: answered like KataGo, an analysis stream ending in `play <move>`.
    analyze: bool,
    color: &'static str,
    sent: bool,
    started: bool,
    final_received: bool,
    stop_ids: Vec<String>,
    terminate_ack: bool,
}

pub(super) struct Adapter {
    width: usize,
    height: usize,
    komi: f64,
    rules: String,
    initial_stones: Vec<Move>,
    moves: Vec<Move>,
    initial_player: Option<&'static str>,
    max_time: f64,
    max_visits: u64,
    advantage: f64,
    noise: f64,
    sequence: u64,
    active: Option<Query>,
    /// Analysis streams a later command ended while an explicit `stop` of theirs waits for the
    /// final result and the matching terminate ack (Java `completeStoppedQueryIfReady`).
    stopping: Vec<Query>,
    /// Those `stop`s, settled while the active query's response was open: answered once it is
    /// complete, as a GTP response cannot start inside another.
    settled: Vec<String>,
}

impl Default for Adapter {
    fn default() -> Self {
        Self {
            width: 19, height: 19, komi: 7.5,
            rules: KataRules::preset("chinese").unwrap().to_gtp_argument(),
            initial_stones: Vec::new(), moves: Vec::new(), initial_player: None,
            max_time: 1e20, max_visits: 256, advantage: 0.0, noise: 0.04,
            sequence: 0, active: None, stopping: Vec::new(), settled: Vec::new(),
        }
    }
}

pub(super) fn command_parts(line: &str) -> (&str, &str) {
    let line = line.trim();
    match line.split_once(char::is_whitespace) {
        Some((id, command)) if !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()) => (id, command.trim()),
        _ => ("", line),
    }
}

fn stdout(output: &Output, text: &str) {
    for line in text.split_terminator('\n') {
        output.emit(EngineInput::Stdout(line.into()));
    }
}

fn ok(output: &Output, id: &str, body: &str) {
    stdout(output, &format!("={id}{}\n\n", if body.is_empty() { String::new() } else { format!(" {body}") }));
}

fn error(output: &Output, id: &str, message: &str) {
    stdout(output, &format!("?{id} {message}\n\n"));
}

fn color(value: &str) -> Option<&'static str> {
    match value.as_bytes().first().map(u8::to_ascii_uppercase) {
        Some(b'B') => Some("B"), Some(b'W') => Some("W"), _ => None,
    }
}

fn terminate(query_id: &str) -> String {
    json!({ "id": format!("terminate-{query_id}"), "action": "terminate", "terminateId": query_id }).to_string()
}

impl Adapter {
    pub(super) fn command(&mut self, raw: &str, output: &Output) -> Result<Action, &'static str> {
        let (id, command) = command_parts(raw);
        if command.is_empty() { return Ok(Action::None); }
        let tokens: Vec<&str> = command.split_whitespace().collect();
        let name = tokens[0].to_ascii_lowercase();
        let token = |i: usize| tokens.get(i).copied().unwrap_or("");
        // Any other command ends a running analysis stream first, as in KataGo's GTP loop.
        let mut sends: Vec<String> = if matches!(name.as_str(), "stop" | "stop-ponder" | "quit") {
            Vec::new()
        } else {
            self.end_stream(output).into_iter().collect()
        };
        let mut body = String::new();
        let result: Result<(), &'static str> = match name.as_str() {
            "name" => { body = "KataGo".into(); Ok(()) }
            "protocol_version" => { body = "2".into(); Ok(()) }
            "version" => { body = "1.16.4".into(); Ok(()) }
            "list_commands" => { body = COMMANDS.into(); Ok(()) }
            "kata-list_time_settings" => { body = "none".into(); Ok(()) }
            "boardsize" | "rectangular_boardsize" => {
                let width = token(1).parse::<i64>().unwrap_or(19).max(1);
                let height = if name == "boardsize" { width } else { token(2).parse::<i64>().unwrap_or(19).max(1) };
                if width > u16::MAX as i64 || height > u16::MAX as i64 {
                    Err("unsupported board size")
                } else {
                    self.width = width as usize; self.height = height as usize;
                    self.clear(); Ok(())
                }
            }
            "komi" => {
                if let Ok(komi) = token(1).parse::<f64>() {
                    if komi.is_finite() { self.komi = komi; }
                }
                Ok(())
            }
            "kata-get-rules" => { body = self.rules.clone(); Ok(()) }
            "kata-set-rules" => {
                let value = command.split_once(char::is_whitespace).map_or("", |(_, value)| value.trim());
                match KataRules::parse(value) {
                    Some(rules) => { self.rules = rules.to_gtp_argument(); Ok(()) }
                    None => Err("unsupported rules"),
                }
            }
            "kata-get-param" => {
                match token(1) {
                    "maxTime" => { body = self.max_time.to_string(); Ok(()) }
                    "maxVisits" => { body = self.max_visits.to_string(); Ok(()) }
                    "playoutDoublingAdvantage" => { body = self.advantage.to_string(); Ok(()) }
                    "analysisWideRootNoise" => { body = self.noise.to_string(); Ok(()) }
                    _ => Err("unknown parameter"),
                }
            }
            "kata-set-param" => self.set_param(token(1), token(2), tokens.len() > 3),
            "kata-time_settings" if tokens.len() == 2 && token(1).eq_ignore_ascii_case("none") => Ok(()),
            "kata-time_settings" => Err("websocket analysis adapter supports only kata-time_settings none"),
            "time_settings" => Err("time_settings unsupported by websocket analysis adapter"),
            "time_left" => Err("time_left unsupported by websocket analysis adapter"),
            "clear_board" => { self.clear(); Ok(()) }
            "set_position" => self.set_position(&tokens[1..]),
            "loadsgf" => {
                // Runtime owns the snapshot file, including its removal after the GTP response.
                let path = command.split_once(char::is_whitespace).map_or("", |(_, path)| path.trim());
                let path = path.strip_prefix('"').and_then(|p| p.strip_suffix('"')).unwrap_or(path);
                match std::fs::read_to_string(path) {
                    Ok(sgf) => self.load_snapshot(&sgf),
                    Err(_) => Err("could not read loadsgf snapshot"),
                }
            }
            "play" => {
                match color(token(1)) {
                    Some(color) if !token(2).is_empty() => {
                        self.moves.push(Move { color, point: token(2).into() }); Ok(())
                    }
                    _ => Err("invalid play command"),
                }
            }
            "undo" => { self.moves.pop(); Ok(()) }
            "kata-raw-nn" => Err("kata-raw-nn unsupported by websocket analysis adapter"),
            "kata-analyze" | "kata-genmove_analyze" | "genmove" => {
                if self.active.is_some() {
                    Err("another websocket analysis query is still active")
                } else if tokens.iter().any(|t| t.eq_ignore_ascii_case("focus")) {
                    // Analysis queries have no focus field: say so rather than search without it.
                    Err("unknown analyze option focus")
                } else {
                    let genmove = name != "kata-analyze";
                    let player = color(token(1)).unwrap_or_else(|| self.next_player());
                    let interval_index = if color(token(1)).is_some() { 2 } else { 1 };
                    let interval = token(interval_index).parse::<i32>().unwrap_or(50).max(10);
                    let ownership = command.to_ascii_lowercase().contains("ownership true");
                    self.sequence += 1;
                    let query_id = format!("lz-ws-{}", self.sequence);
                    let payload = self.build_query(&query_id, genmove, player, interval, ownership);
                    self.active = Some(Query {
                        sequence: self.sequence, id: query_id, response_id: id.into(), genmove,
                        analyze: name == "kata-genmove_analyze", color: player, sent: false, started: false,
                        final_received: false, stop_ids: Vec::new(), terminate_ack: false,
                    });
                    sends.push(payload.to_string());
                    return Ok(Action::Send(sends));
                }
            }
            "stop" | "stop-ponder" => {
                let Some(query) = self.active.as_mut() else { ok(output, id, ""); return Ok(Action::None); };
                query.stop_ids.push(id.into());
                if query.stop_ids.len() == 1 {
                    return Ok(Action::Send(vec![terminate(&query.id)]));
                }
                return Ok(Action::None);
            }
            "quit" => { ok(output, id, ""); self.active = None; return Ok(Action::Quit); }
            _ => Err("unknown command"),
        };
        match result {
            Ok(()) => ok(output, id, &body),
            Err(message) => error(output, id, message),
        }
        Ok(if sends.is_empty() { Action::None } else { Action::Send(sends) })
    }

    /// Ends the active analysis stream: its response closes and the remote query is terminated
    /// unless a `stop` already did; its late results stay inert. A pending `stop` is answered only
    /// once that query's final result and terminate ack arrive. Returns that `terminate`. A move
    /// query is not a stream and keeps running.
    fn end_stream(&mut self, output: &Output) -> Option<String> {
        let query = self.active.take_if(|query| !query.genmove)?;
        if query.started && !query.final_received { stdout(output, "\n"); }
        self.answer_settled(output);
        if query.stop_ids.is_empty() {
            return Some(terminate(&query.id));
        }
        self.stopping.push(query);
        None
    }

    /// A response for a stopped stream a later command ended: it settles that `stop` once its
    /// final result and matching ack are in, answered at once unless the active query's response
    /// is open. A remote error fails the transport, as Java's `failActiveQuery`; the `stop` is
    /// never answered with success.
    fn stopping_response(&mut self, at: usize, id: &str, response: &Value, output: &Output) -> Result<(), ()> {
        if response.get("error").is_some() { return Err(()); }
        let query = &mut self.stopping[at];
        if id == format!("terminate-{}", query.id) {
            if response["action"] != "terminate" || response["terminateId"] != query.id { return Err(()); }
            query.terminate_ack = true;
        } else if response.get("warning").is_some() {
            output.emit(EngineInput::Stderr("Custom cloud analysis warning".into()));
        } else if !response["isDuringSearch"].as_bool().ok_or(())? {
            query.final_received = true;
        }
        if query.final_received && query.terminate_ack {
            let stops = self.stopping.remove(at).stop_ids;
            self.settled.extend(stops);
            if !self.active.as_ref().is_some_and(|active| active.started && !active.final_received) {
                self.answer_settled(output);
            }
        }
        Ok(())
    }

    fn answer_settled(&mut self, output: &Output) {
        for id in self.settled.drain(..) { ok(output, &id, ""); }
    }

    fn clear(&mut self) {
        self.initial_stones.clear(); self.moves.clear(); self.initial_player = None;
    }

    fn next_player(&self) -> &'static str {
        let first = self.initial_player.unwrap_or("B");
        if self.moves.len() % 2 == 0 { first } else if first == "B" { "W" } else { "B" }
    }

    fn set_param(&mut self, param: &str, value: &str, extra: bool) -> Result<(), &'static str> {
        if extra { return Err("invalid parameter value"); }
        let (min, max, default) = match param {
            "maxTime" => (0.0, 1e20, Some(1e20)),
            "maxVisits" => (1.0, MAX_VISITS, Some(256.0)),
            "playoutDoublingAdvantage" => (-3.0, 3.0, None),
            "analysisWideRootNoise" => (0.0, 5.0, None),
            _ => return Err("unknown parameter"),
        };
        let value = if value.is_empty() { default.ok_or("invalid parameter value")? } else {
            value.parse::<f64>().map_err(|_| "invalid parameter value")?
        };
        if !value.is_finite() || value < min || value > max || (param == "maxVisits" && value.fract() != 0.0) {
            return Err("invalid parameter value");
        }
        match param {
            "maxTime" => self.max_time = value,
            "maxVisits" => self.max_visits = value as u64,
            "playoutDoublingAdvantage" => self.advantage = value,
            "analysisWideRootNoise" => self.noise = value,
            _ => unreachable!(),
        }
        Ok(())
    }

    fn build_query(&self, id: &str, genmove: bool, player: &str, interval: i32, ownership: bool) -> Value {
        let initial: Vec<Value> = self.initial_stones.iter().map(|m| json!([m.color, m.point])).collect();
        let moves: Vec<Value> = self.moves.iter().map(|m| json!([m.color, m.point])).collect();
        let mut overrides = json!({ "reportAnalysisWinratesAs": if player == "B" { "BLACK" } else { "WHITE" } });
        overrides[if genmove { "maxTime" } else { "wideRootNoise" }] = json!(if genmove { self.max_time } else { self.noise });
        let mut query = json!({
            "id": id, "rules": serde_json::from_str::<Value>(&self.rules).unwrap(),
            "priority": if genmove { 2 } else { 0 }, "analyzeTurns": [self.moves.len()],
            "maxVisits": if genmove { self.max_visits } else { 10_000_000 },
            "playoutDoublingAdvantage": self.advantage,
            "komi": self.komi, "boardXSize": self.width, "boardYSize": self.height,
            "includeOwnership": ownership && !genmove, "includeMovesOwnership": ownership && !genmove,
            "includePolicy": true, "initialStones": initial, "moves": moves,
            "initialPlayer": self.moves.first().map(|m| m.color).unwrap_or(self.initial_player.unwrap_or(player)),
            "overrideSettings": overrides,
        });
        if !genmove { query["reportDuringSearchEvery"] = json!((interval as f64 / 100.0).max(0.1)); }
        query
    }

    pub(super) fn sent(&mut self, output: &Output) {
        if let Some(query) = self.active.as_mut() {
            if !query.sent {
                query.sent = true;
                if !query.genmove || query.analyze {
                    stdout(output, &format!("={}\n", query.response_id));
                    query.started = true;
                }
            }
        }
    }

    pub(super) fn response(&mut self, text: &str, output: &Output) -> Result<(), ()> {
        let response: Value = serde_json::from_str(text).map_err(|_| ())?;
        if !response.is_object() { return Err(()); }
        let id = response["id"].as_str().unwrap_or("");
        let sequence = id.strip_prefix("terminate-").unwrap_or(id).strip_prefix("lz-ws-").and_then(|id| id.parse::<u64>().ok());
        if let Some(at) = self.stopping.iter().position(|query| Some(query.sequence) == sequence) {
            return self.stopping_response(at, id, &response, output);
        }
        // Every other response from an old query is inert, including its late warning, error and stop ack.
        if sequence.is_some_and(|sequence| sequence > 0 && sequence <= self.sequence && self.active.as_ref().is_none_or(|active| sequence < active.sequence)) {
            return Ok(());
        }
        let Some(query) = self.active.as_mut() else { return Err(()); };
        if response.get("error").is_some() {
            if !query.started { error(output, &query.response_id, "custom cloud rejected analysis query"); }
            self.active = None;
            return Err(());
        }
        if id == format!("terminate-{}", query.id) {
            if query.stop_ids.is_empty() || response["action"] != "terminate" || response["terminateId"] != query.id { return Err(()); }
            query.terminate_ack = true;
        } else {
            if id != query.id || !query.sent { return Err(()); }
            if response.get("warning").is_some() {
                output.emit(EngineInput::Stderr("Custom cloud analysis warning".into()));
                return Ok(());
            }
            if query.final_received { return Ok(()); }
            let partial = response["isDuringSearch"].as_bool().ok_or(())?;
            let line = info_line(&response, self.width * self.height);
            if !line.is_empty() { stdout(output, &format!("{line}\n")); }
            if !partial {
                if query.genmove {
                    let point = response["moveInfos"].as_array().and_then(|infos| infos.first()).and_then(|info| info["move"].as_str()).filter(|point| !point.is_empty()).ok_or(())?;
                    self.moves.push(Move { color: query.color, point: point.into() });
                    if query.analyze {
                        stdout(output, &format!("play {point}\n\n"));
                    } else {
                        ok(output, &query.response_id, point);
                    }
                } else {
                    stdout(output, "\n");
                }
                query.final_received = true;
            }
        }
        if query.final_received {
            // Its response is complete: `stop`s settled meanwhile come first.
            for id in self.settled.drain(..) { ok(output, &id, ""); }
            if query.stop_ids.is_empty() || query.terminate_ack {
                for id in &query.stop_ids { ok(output, id, ""); }
                self.active = None;
            }
        }
        Ok(())
    }

    fn set_position(&mut self, tokens: &[&str]) -> Result<(), &'static str> {
        if tokens.len() % 2 != 0 { return Err("invalid set_position command"); }
        let mut occupied = HashMap::new();
        let mut stones = Vec::new();
        for pair in tokens.chunks_exact(2) {
            let color = color(pair[0]).ok_or("invalid set_position command")?;
            let point = pair[1].to_ascii_uppercase();
            let xy = coordinates(&point, self.width, self.height).ok_or("invalid set_position command")?;
            if occupied.insert(xy, color).is_some() { return Err("invalid set_position command"); }
            stones.push(Move { color, point });
        }
        if zero_liberty(&occupied, self.width, self.height) { return Err("invalid set_position command"); }
        self.initial_stones = stones; self.moves.clear(); self.initial_player = None;
        Ok(())
    }

    fn load_snapshot(&mut self, sgf: &str) -> Result<(), &'static str> {
        static SNAPSHOT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\(;FF\[4\]GM\[1\]CA\[UTF-8\]SZ\[(\d+(?::\d+)?)\](?:KM\[([-+]?(?:\d+(?:\.\d*)?|\.\d+))\])?PL\[([BW])\]((?:AB\[[^\]]+\])*(?:AW\[[^\]]+\])*)\)$").unwrap());
        static STONE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(AB|AW)\[([^\]]+)\]").unwrap());
        let pattern = &*SNAPSHOT;
        let stone_pattern = &*STONE;
        let captures = pattern.captures(sgf.trim()).map_err(|_| "unsupported loadsgf snapshot")?.ok_or("unsupported loadsgf snapshot")?;
        let size: Vec<&str> = captures[1].split(':').collect();
        let width = size[0].parse::<usize>().map_err(|_| "invalid loadsgf board size")?;
        let height = size.get(1).copied().unwrap_or(size[0]).parse::<usize>().map_err(|_| "invalid loadsgf board size")?;
        if width == 0 || height == 0 || width > u16::MAX as usize || height > u16::MAX as usize { return Err("invalid loadsgf board size"); }
        let komi = captures.get(2).map(|m| m.as_str().parse::<f64>()).transpose().map_err(|_| "invalid loadsgf komi")?;
        if komi.is_some_and(|komi| !komi.is_finite()) { return Err("invalid loadsgf komi"); }
        let mut occupied = HashSet::new();
        let mut stones = Vec::new();
        for stone in stone_pattern.captures_iter(&captures[4]) {
            let stone = stone.map_err(|_| "invalid loadsgf snapshot")?;
            let (x, y) = snapshot_xy(&stone[2], width, height).ok_or("invalid loadsgf coordinate")?;
            if !occupied.insert((x, y)) { return Err("invalid loadsgf snapshot"); }
            stones.push(Move { color: if &stone[1] == "AB" { "B" } else { "W" }, point: vertex(x, y, width, height) });
        }
        self.width = width; self.height = height;
        if let Some(komi) = komi { self.komi = komi; }
        self.initial_stones = stones; self.moves.clear(); self.initial_player = color(&captures[3]);
        Ok(())
    }
}

fn snapshot_xy(value: &str, width: usize, height: usize) -> Option<(usize, usize)> {
    let xy = if width >= 52 || height >= 52 {
        let (x, y) = value.split_once('_')?;
        (x.parse().ok()?, y.parse().ok()?)
    } else {
        let mut chars = value.chars();
        let alphabet = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let x = alphabet.find(chars.next()?)?;
        let y = alphabet.find(chars.next()?)?;
        if chars.next().is_some() { return None; }
        (x, y)
    };
    (xy.0 < width && xy.1 < height).then_some(xy)
}

fn vertex(x: usize, y: usize, width: usize, height: usize) -> String {
    if width > 25 || height > 25 { format!("({x},{y})") } else {
        format!("{}{}", (b'A' + x as u8 + u8::from(x >= 8)) as char, height - y)
    }
}

fn coordinates(point: &str, width: usize, height: usize) -> Option<(usize, usize)> {
    if let Some(tuple) = point.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
        let (x, y) = tuple.split_once(',')?;
        let xy = (x.parse::<usize>().ok()?, y.parse::<usize>().ok()?);
        return (xy.0 < width && xy.1 < height).then_some(xy);
    }
    let column = *point.as_bytes().first()?;
    if !column.is_ascii_uppercase() || column == b'I' { return None; }
    let x = (column - b'A' - u8::from(column > b'I')) as usize;
    let row = point.get(1..)?.parse::<usize>().ok()?;
    (x < width && row > 0 && row <= height).then(|| (x, height - row))
}

fn zero_liberty(stones: &HashMap<(usize, usize), &'static str>, width: usize, height: usize) -> bool {
    let mut visited = HashSet::new();
    for (&start, &color) in stones {
        if !visited.insert(start) { continue; }
        let mut group = vec![start];
        let mut liberty = false;
        while let Some((x, y)) = group.pop() {
            for adjacent in [x.checked_sub(1).map(|x| (x, y)), y.checked_sub(1).map(|y| (x, y)), (x + 1 < width).then_some((x + 1, y)), (y + 1 < height).then_some((x, y + 1))].into_iter().flatten() {
                match stones.get(&adjacent) {
                    None => liberty = true,
                    Some(&other) if other == color && visited.insert(adjacent) => group.push(adjacent),
                    _ => {},
                }
            }
        }
        if !liberty { return true; }
    }
    false
}

fn append_number(line: &mut String, object: &Value, source: &str, target: &str) {
    if let Some(value) = object[source].as_f64().filter(|value| value.is_finite()) {
        line.push_str(&format!(" {target} {value:.6}"));
    }
}

fn append_array(line: &mut String, value: &Value, tag: &str, limit: usize) {
    if let Some(values) = value.as_array().filter(|values| !values.is_empty()) {
        line.push(' '); line.push_str(tag);
        for value in values.iter().take(limit) {
            line.push(' ');
            if let Some(point) = value.as_str() { line.push_str(point); }
            else { line.push_str(&value.to_string()); }
        }
    }
}

fn info_line(response: &Value, points: usize) -> String {
    let mut line = String::new();
    if let Some(moves) = response["moveInfos"].as_array() {
        for (order, info) in moves.iter().enumerate().filter(|(_, info)| info.is_object()) {
            if !line.is_empty() { line.push(' '); }
            line.push_str("info move ");
            line.push_str(info["move"].as_str().unwrap_or("pass"));
            line.push_str(&format!(" visits {}", info["visits"].as_u64().unwrap_or(0)));
            if let Some(visits) = info["edgeVisits"].as_u64() { line.push_str(&format!(" edgeVisits {visits}")); }
            if info.get("isSymmetryOf").is_some() { line.push_str(" isSymmetryOf true"); }
            for (source, target) in [("winrate", "winrate"), ("prior", "prior"), ("lcb", "lcb"), ("scoreStdev", "scoreStdev")] {
                append_number(&mut line, info, source, target);
            }
            append_number(&mut line, info, if info.get("scoreLead").is_some() { "scoreLead" } else { "scoreMean" }, "scoreLead");
            line.push_str(&format!(" order {}", info["order"].as_u64().unwrap_or(order as u64)));
            append_array(&mut line, &info["pv"], "pv", usize::MAX);
            append_array(&mut line, &info["pvVisits"], "pvVisits", usize::MAX);
            append_array(&mut line, &info["ownership"], "movesOwnership", points);
        }
    }
    if response["rootInfo"].is_object() {
        if line.is_empty() { line.push_str("info"); }
        line.push_str(" rootInfo");
        for field in ["visits", "winrate", "scoreLead"] {
            // Visits must remain an integer for the GTP parser.
            if field == "visits" {
                if let Some(visits) = response["rootInfo"][field].as_u64() { line.push_str(&format!(" visits {visits}")); }
            } else { append_number(&mut line, &response["rootInfo"], field, field); }
        }
    }
    if !line.is_empty() { append_array(&mut line, &response["ownership"], "ownership", points); }
    line
}

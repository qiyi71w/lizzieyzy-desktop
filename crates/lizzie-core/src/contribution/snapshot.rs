//! KataGo `contribute` game snapshots (`logGamesAsJson=true`): one JSON object per turn, parsed
//! exactly like Java `ContributeEngine.getJsonGameInfo`, `tryToParseJsonGame`,
//! `tryToUseUnParseGameInfos`, `compareMoveList` and `getModelName`.
//!
//! Win rate and score lead are reported from Black's perspective and are turned to the side to
//! move here, like Java `Utils.getBestMovesFromJsonArray(moveInfos, false, isBlack)`; ownership
//! stays Black-positive.

use serde_json::Value;

use crate::record::{Point, Stone};

/// One move of a game's history or its newest analysed move.
#[derive(Clone, Debug, PartialEq)]
pub struct Move {
    pub black: bool,
    pub pass: bool,
    pub point: Option<Point>,
    /// The search of the position *before* this move, when the line reports one.
    pub analysis: Option<Analysis>,
}

/// The `moveInfos` (and optional `ownership`) of the position before a move, side to move.
#[derive(Clone, Debug, PartialEq)]
pub struct Analysis {
    pub to_play: Stone,
    pub winrate: f64,
    pub score_lead: Option<f64>,
    pub score_stdev: Option<f64>,
    pub candidates: Vec<Candidate>,
    /// Black-positive ownership of the position, like `BoardData.estimateArray`.
    pub ownership: Option<Vec<f64>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub point: Option<Point>,
    pub visits: u64,
    /// `edgeVisits` when reported, else `visits`.
    pub allocation_visits: u64,
    pub winrate: f64,
    pub score_lead: Option<f64>,
    pub score_stdev: Option<f64>,
    pub prior: f64,
    pub lcb: f64,
    pub order: u32,
    pub pv: Vec<Option<Point>>,
    pub pv_visits: Vec<u64>,
    /// KataGo `ownership` after this move, Black-positive.
    pub ownership: Option<Vec<f64>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    pub id: String,
    pub black: String,
    pub white: String,
    pub width: u16,
    pub height: u16,
    pub komi: f64,
    pub rules: Value,
    pub init: Vec<Move>,
    pub moves: Vec<Move>,
    pub complete: bool,
    pub result: String,
    pub rating: bool,
    pub saved: bool,
}

/// A snapshot that arrived before its game's history was known
/// (`ContributeUnParseGameInfo`).
#[derive(Clone, Debug, PartialEq)]
pub struct Unparsed {
    pub game_id: String,
    pub json: Value,
    pub used: bool,
    pub skip: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ingest {
    Updated(usize),
    Added(usize),
    Ignored,
}

/// The five game fields Java requires before it parses a snapshot.
pub fn has_snapshot(json: &Value) -> bool {
    ["gameId", "initialStones", "moves", "move", "moveInfos"]
        .into_iter()
        .all(|key| json.get(key).is_some())
}

/// `getModelName`: the model after the first `–`, without the `-d…` snapshot suffix.
pub fn model_name(name: &str) -> String {
    match name.find('-') {
        Some(at) => {
            let rest = &name[at + 1..];
            match rest.find("-d") {
                Some(suffix) => rest[..suffix].to_string(),
                None => rest.to_string(),
            }
        }
        None => name.to_string(),
    }
}

/// `Board.convertNameToCoordinates(name, height)` on the game's own `width`×`height` board:
/// `C16`, `Q5`; `pass`/`resign`, anything unparseable and a point off that board (Java's
/// out-of-bound coordinate) are a pass.
pub fn point_from_name(name: &str, width: u16, height: u16) -> Option<Point> {
    let name = name.trim();
    if name.eq_ignore_ascii_case("pass") || name.eq_ignore_ascii_case("resign") {
        return None;
    }
    if let Some(inner) = name.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
        let (x, y) = inner.split_once(',')?;
        let x: u16 = x.trim().parse().ok()?;
        let y: u16 = y.trim().parse().ok()?;
        return (x < width && y < height).then_some(Point { x, y });
    }
    let mut letters = 0;
    let mut x: usize = 0;
    for c in name.chars() {
        let upper = c.to_ascii_uppercase();
        if upper.is_ascii_uppercase() && upper != 'I' && letters < 2 {
            let value = (upper as u8 - b'A') as usize;
            let value = if value > 8 { value - 1 } else { value };
            x = x * 25 + value;
            letters += 1;
        } else {
            break;
        }
    }
    if letters == 0 {
        return None;
    }
    let rest = &name[name.char_indices().nth(letters).map_or(name.len(), |(i, _)| i)..];
    if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let row: u16 = rest.parse().ok()?;
    let y = height.checked_sub(row)?;
    let x = u16::try_from(x).ok()?;
    (x < width && y < height).then_some(Point { x, y })
}

fn parse_color(value: &Value) -> Option<bool> {
    match value.as_str()? {
        "B" => Some(true),
        "W" => Some(false),
        _ => None,
    }
}

fn parse_move_value(value: &Value, width: u16, height: u16) -> Option<(bool, Option<Point>)> {
    let pair = value.as_array()?;
    if pair.len() < 2 {
        return None;
    }
    let black = parse_color(&pair[0])?;
    let point = pair[1].as_str().and_then(|name| point_from_name(name, width, height));
    Some((black, point))
}

/// `initialStones` / `moves`: `[["B", "Q16"], ...]`, the last entries without analysis.
pub fn parse_moves(value: &Value, width: u16, height: u16) -> Option<Vec<Move>> {
    let array = value.as_array()?;
    let mut moves = Vec::with_capacity(array.len());
    for entry in array {
        let (black, point) = parse_move_value(entry, width, height)?;
        moves.push(Move {
            black,
            pass: point.is_none(),
            point,
            analysis: None,
        });
    }
    Some(moves)
}

fn number(value: &Value, key: &str) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn integer(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn analysis(json: &Value, black: bool, width: u16, height: u16) -> Option<Analysis> {
    let infos = json.get("moveInfos")?.as_array()?;
    let to_play = if black { Stone::Black } else { Stone::White };
    let mut candidates = Vec::with_capacity(infos.len());
    for info in infos {
        let point = info
            .get("move")
            .and_then(Value::as_str)
            .and_then(|name| point_from_name(name, width, height));
        let (mut winrate, mut score_lead); // Black perspective, as reported
        winrate = number(info, "winrate") * 100.0;
        score_lead = info.get("scoreLead").and_then(Value::as_f64);
        if !black {
            winrate = 100.0 - winrate;
            score_lead = score_lead.map(|lead| -lead);
        }
        let edge_visits = info
            .get("edgeVisits")
            .and_then(Value::as_i64)
            .filter(|visits| *visits >= 0)
            .map_or_else(|| integer(info, "visits"), |visits| visits as u64);
        let pv = info
            .get("pv")
            .and_then(Value::as_array)
            .map(|pv| {
                pv.iter()
                    .map(|step| {
                        step.as_str().and_then(|name| point_from_name(name, width, height))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let pv_visits = info
            .get("pvVisits")
            .and_then(Value::as_array)
            .map(|visits| visits.iter().map(|v| v.as_u64().unwrap_or(0)).collect())
            .unwrap_or_default();
        candidates.push(Candidate {
            point,
            visits: integer(info, "visits"),
            allocation_visits: edge_visits,
            winrate,
            score_lead,
            score_stdev: info.get("scoreStdev").and_then(Value::as_f64),
            prior: number(info, "prior") * 100.0,
            lcb: number(info, "lcb") * 100.0,
            order: info.get("order").and_then(Value::as_u64).unwrap_or(0) as u32,
            pv,
            pv_visits,
            ownership: info
                .get("ownership")
                .and_then(Value::as_array)
                .map(|values| values.iter().filter_map(Value::as_f64).collect()),
        });
    }
    let best = candidates.first();
    let ownership = json.get("ownership").and_then(Value::as_array).map(|values| {
        values
            .iter()
            .filter_map(Value::as_f64)
            .map(|v| if black { v } else { -v })
            .collect()
    });
    Some(Analysis {
        to_play,
        winrate: best.map_or(50.0, |c| c.winrate),
        score_lead: best.and_then(|c| c.score_lead),
        score_stdev: best.and_then(|c| c.score_stdev),
        candidates,
        ownership,
    })
}

fn base_game(json: &Value) -> Option<Game> {
    let id = json.get("gameId")?.as_str()?.to_string();
    let black = model_name(json.get("blackPlayer")?.as_str()?);
    let white = model_name(json.get("whitePlayer")?.as_str()?);
    let width = u16::try_from(json.get("boardXSize")?.as_u64()?).ok()?;
    let height = u16::try_from(json.get("boardYSize")?.as_u64()?).ok()?;
    let rules = json.get("rules")?.clone();
    let komi = rules.get("komi")?.as_f64()?;
    let init = parse_moves(json.get("initialStones")?, width, height)?;
    let moves = parse_moves(json.get("moves")?, width, height)?;
    Some(Game {
        id,
        rating: black != white,
        black,
        white,
        width,
        height,
        komi,
        rules,
        init,
        moves,
        complete: false,
        result: String::new(),
        saved: false,
    })
}

/// `tryToParseJsonGame`: appends the newest move (with its analysis) when the reported history
/// matches what this game already knows, or queues the snapshot when it does not.
fn parse_into(game: &mut Game, new_game: bool, json: &Value) -> Option<bool> {
    let history = parse_moves(json.get("moves")?, game.width, game.height)?;
    let same = new_game || compare_moves(&history, &game.moves);
    if same {
        let (black, point) = parse_move_value(json.get("move")?, game.width, game.height)?;
        let analysis = analysis(json, black, game.width, game.height).filter(|a| !a.candidates.is_empty());
        game.moves.push(Move {
            black,
            pass: point.is_none(),
            point,
            analysis,
        });
        Some(true)
    } else {
        Some(false)
    }
}

/// `compareMoveList`: equal lengths and the same colour, pass flag and point at every index.
pub fn compare_moves(list1: &[Move], list2: &[Move]) -> bool {
    list1.len() == list2.len()
        && list1.iter().zip(list2).all(|(a, b)| {
            a.black == b.black && a.pass == b.pass && a.point == b.point
        })
}

/// `tryToUseUnParseGameInfos`: the first queued snapshot of this game that fits now.
fn use_unparsed(game: &mut Game, unparsed: &mut Vec<Unparsed>) {
    let Some(index) = unparsed
        .iter()
        .position(|u| u.game_id == game.id && !u.used && !u.skip)
    else {
        return;
    };
    let json = unparsed[index].json.clone();
    if parse_into(game, false, &json) == Some(true) {
        unparsed.remove(index);
        for entry in unparsed.iter_mut() {
            if entry.game_id == game.id {
                entry.skip = false;
            }
        }
    } else {
        unparsed[index].skip = true;
        use_unparsed(game, unparsed);
    }
}

/// `getJsonGameInfo`: updates the game a snapshot belongs to, adds a new one, or queues it.
pub fn ingest(games: &mut Vec<Game>, unparsed: &mut Vec<Unparsed>, json: &Value) -> Ingest {
    let Some(id) = json.get("gameId").and_then(Value::as_str) else {
        return Ingest::Ignored;
    };
    if let Some(index) = games.iter().position(|game| game.id == id) {
        let mut game = games[index].clone();
        match parse_into(&mut game, false, json) {
            Some(true) => {
                use_unparsed(&mut game, unparsed);
                games[index] = game;
                Ingest::Updated(index)
            }
            Some(false) => {
                if let Some(queued) = parse_queue_snapshot(json) {
                    unparsed.push(queued);
                }
                Ingest::Ignored
            }
            None => Ingest::Ignored,
        }
    } else {
        let Some(mut game) = base_game(json) else {
            return Ingest::Ignored;
        };
        if parse_into(&mut game, true, json) == Some(true) {
            games.push(game);
            Ingest::Added(games.len() - 1)
        } else {
            Ingest::Ignored
        }
    }
}

/// `tryToParseJsonGame`'s queueing branch (`newGame == false`, mismatched history).
fn parse_queue_snapshot(json: &Value) -> Option<Unparsed> {
    Some(Unparsed {
        game_id: json.get("gameId")?.as_str()?.to_string(),
        json: json.clone(),
        used: false,
        skip: false,
    })
}

/// The result read from a finished game's SGF (`SGFParser.parseResult`): the first 20 characters
/// after `RE[` up to `]`.
pub fn parse_result(text: &str) -> String {
    let Some(start) = text.find("RE[") else {
        return String::new();
    };
    let rest = &text[start + 3..];
    let window: String = rest.chars().take(20).collect();
    match window.find(']') {
        Some(end) => window[..end].to_string(),
        None => window,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn names_and_models_follow_java() {
        assert_eq!(model_name("kata1-b18c384nbt-s999-d1234"), "b18c384nbt-s999");
        assert_eq!(model_name("plain"), "plain");
    }

    #[test]
    fn coordinates_are_gtp_columns_without_i() {
        assert_eq!(point_from_name("Q16", 19, 19), Some(Point { x: 15, y: 3 }));
        assert_eq!(point_from_name("A19", 19, 19), Some(Point { x: 0, y: 0 }));
        assert_eq!(point_from_name("J1", 19, 19), Some(Point { x: 8, y: 18 }));
        assert_eq!(point_from_name("pass", 19, 19), None);
        assert_eq!(point_from_name("Z99", 19, 19), None);
        // A 9×9 game: its own rows, and a 19×19 name is off the board.
        assert_eq!(point_from_name("E5", 9, 9), Some(Point { x: 4, y: 4 }));
        assert_eq!(point_from_name("Q3", 9, 9), None);
    }

    #[test]
    fn snapshots_are_turned_to_the_side_to_move() {
        let json = json!({
            "gameId": "g1",
            "blackPlayer": "kata1-b18c384nbt-s1-d2",
            "whitePlayer": "kata1-b18c384nbt-s1-d2",
            "boardXSize": 19, "boardYSize": 19,
            "rules": {"komi": 7.0, "scoring": "AREA"},
            "initialStones": [],
            "moves": [["B", "Q16"]],
            "move": ["W", "D4"],
            "moveInfos": [
                {"move": "D4", "visits": 100, "winrate": 0.6, "scoreLead": 2.0, "scoreStdev": 3.0,
                 "prior": 0.3, "lcb": 0.5, "order": 0, "pv": ["D4", "Q3"]},
                {"move": "Q3", "visits": 40, "winrate": 0.55, "scoreLead": 1.0, "scoreStdev": 3.0,
                 "prior": 0.2, "lcb": 0.4, "order": 1, "pv": ["Q3"]}
            ],
            "ownership": [0.5, -0.5]
        });
        let mut games = Vec::new();
        let mut unparsed = Vec::new();
        assert_eq!(ingest(&mut games, &mut unparsed, &json), Ingest::Added(0));
        let game = &games[0];
        assert_eq!(game.moves.len(), 2);
        assert_eq!(game.black, "b18c384nbt-s1");
        assert!(!game.rating);
        let analysis = game.moves[1].analysis.as_ref().unwrap();
        assert_eq!(analysis.to_play, Stone::White);
        assert!((analysis.winrate - 40.0).abs() < 1e-9);
        assert_eq!(analysis.score_lead, Some(-2.0));
        assert_eq!(analysis.ownership, Some(vec![-0.5, 0.5]));
        assert!((analysis.candidates[0].prior - 30.0).abs() < 1e-9);
        assert_eq!(analysis.candidates[0].pv, vec![Some(Point { x: 3, y: 15 }), Some(Point { x: 15, y: 16 })]);
    }

    #[test]
    fn a_snapshot_whose_history_is_not_known_yet_is_guarded_then_used() {
        let early = json!({
            "gameId": "g1", "blackPlayer": "b", "whitePlayer": "w",
            "boardXSize": 19, "boardYSize": 19, "rules": {"komi": 7.0},
            "initialStones": [], "moves": [["B", "Q16"]],
            "move": ["W", "D4"], "moveInfos": [{"move": "D4", "visits": 10, "winrate": 0.5,
                "scoreLead": 0.0, "scoreStdev": 1.0, "prior": 0.1, "lcb": 0.4, "order": 0, "pv": ["D4"]}]
        });
        let mut games = vec![Game {
            id: "g1".into(), black: "b".into(), white: "w".into(), width: 19, height: 19,
            komi: 7.0, rules: json!({"komi": 7.0}), init: vec![], moves: vec![], complete: false,
            result: String::new(), rating: true, saved: false,
        }];
        let mut unparsed = Vec::new();
        assert_eq!(ingest(&mut games, &mut unparsed, &early), Ingest::Ignored);
        assert_eq!(unparsed.len(), 1);
        // The first line of the game confirms the (still empty) prefix; the queued snapshot is
        // then used at once, appending the move it carried.
        let base = json!({
            "gameId": "g1", "blackPlayer": "b", "whitePlayer": "w",
            "boardXSize": 19, "boardYSize": 19, "rules": {"komi": 7.0},
            "initialStones": [], "moves": [],
            "move": ["B", "Q16"], "moveInfos": []
        });
        assert_eq!(ingest(&mut games, &mut unparsed, &base), Ingest::Updated(0));
        assert!(unparsed.is_empty());
        assert_eq!(games[0].moves.len(), 2);
        assert_eq!(games[0].moves[1].point, Some(Point { x: 3, y: 15 }));
        assert!(games[0].moves[1].analysis.is_some());
    }

    #[test]
    fn the_result_reader_takes_at_most_twenty_characters() {
        assert_eq!(parse_result("...RE[B+R]..."), "B+R");
        assert_eq!(parse_result("no result here"), "");
        assert_eq!(parse_result("RE[W+123456789012345678901234]"), "W+123456789012345678");
    }
}

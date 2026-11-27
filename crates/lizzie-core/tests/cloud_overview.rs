//! 整盘速览 on a borrowed 智子 engine (Java `AnalysisEngine.usesManualZhiziOverview`,
//! `GtpAnalysisProtocol.advanceCommand`, `REMOTE_GTP_PREPARATION_WINDOW`). The workbench talks
//! through the product `CloudConnector` to a loopback Socket.IO peer; its GTP engine is scripted
//! here, and the test decides when and in which order each answer is written.

#[allow(dead_code)]
#[path = "cloud_transport/workbench.rs"]
mod workbench;

use std::collections::VecDeque;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use lizzie_core::api::{FlashOutcome, FlashTarget};
use lizzie_core::browse::Navigation;
use lizzie_core::cloud::{CloudSettings, Provider};
use lizzie_core::engine::cloud::{CloudConnector, SocketCredentials};
use lizzie_core::engine::gtp::parse_vertex;
use lizzie_core::record::{BoardSize, Point, Stone};
use lizzie_core::workbench::Workbench;
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::{self, accept, Message};
use workbench::Bench;

/// How the engine writes the answer to one command.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Answer {
    /// At once: an open analysis ends, then the answer.
    Now,
    /// Not until the test releases it; everything written after it waits too.
    Held,
    /// An open analysis ends at once; the answer waits for the test.
    EndThenHeld,
    /// The answer comes while the analysis is still open; its end waits for the test.
    AnswerThenHeld,
    /// Not until the next command arrives.
    UntilNext,
}

/// Decides the answer from the command (without its id) and whether an analysis was open.
type Policy = Box<dyn FnMut(&str, bool) -> Answer + Send>;

const COMMANDS: &str = "protocol_version\nname\nversion\nknown_command\nlist_commands\nboardsize\nrectangular_boardsize\nclear_board\nkomi\nplay\nundo\nset_position\nkata-get-rules\nkata-set-rules\nkata-analyze\nstop\nquit";
const RULES: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":true,"tax":"NONE","whiteHandicapBonus":"0"}"#;

/// A KataGo stand-in. Each `kata-analyze` reports once: its best move names the number of stones
/// and moves on the board (see [`code`]). A borrowed search's visits are 1000 + its GTP id, so a
/// stored result tells which position and which search it came from; the foreground engine's own
/// analysis reports 1 visit, too few for 整盘速览 to take its position as analysed. The stream
/// stays open until the next command, as KataGo's does.
struct Engine {
    stones: usize,
    moves: Vec<String>,
    analysing: bool,
    /// Every command line received, in order.
    log: Vec<String>,
    /// Each search: its GTP id and the position it searched.
    searches: Vec<(u64, usize)>,
    /// A search was asked for while a command's answer was still held back.
    searched_unanswered: bool,
    out: Vec<String>,
    /// Output lines put out so far (each `\n` in `out` once it is there).
    written: usize,
    held: VecDeque<String>,
    /// The commands whose answers are held back.
    held_commands: Vec<String>,
    until_next: bool,
    policy: Policy,
    /// Moves (`W D16`) refused as illegal, and `stop` refused (once per listing).
    refuse: Vec<String>,
}

impl Engine {
    fn command(&mut self, line: &str) {
        self.log.push(line.to_string());
        if std::mem::take(&mut self.until_next) {
            self.release();
        }
        let (id, command) = line.split_once(' ').expect("numbered command");
        let id: u64 = id.parse().unwrap();
        let ended = std::mem::take(&mut self.analysing).then(|| "\n".to_string());
        let answer = (self.policy)(command, ended.is_some());
        if command.starts_with("kata-analyze") && !self.held.is_empty() {
            self.searched_unanswered = true;
        }
        let reply = Some(self.execute(id, command));
        let held = !matches!(answer, Answer::Now);
        match answer {
            Answer::Now => {
                self.emit(ended);
                self.emit(reply);
            }
            Answer::Held | Answer::UntilNext => {
                self.hold(ended);
                self.hold(reply);
            }
            Answer::EndThenHeld => {
                self.emit(ended);
                self.hold(reply);
            }
            Answer::AnswerThenHeld => {
                self.emit(reply);
                self.hold(ended);
            }
        }
        if held {
            self.held_commands.push(command.to_string());
        }
        self.until_next = answer == Answer::UntilNext;
    }

    fn execute(&mut self, id: u64, command: &str) -> String {
        let mut words = command.split_whitespace();
        let name = words.next().unwrap();
        let args: Vec<&str> = words.collect();
        let ok = |text: &str| format!("={id}{}{text}\n\n", if text.is_empty() { "" } else { " " });
        match name {
            "name" => ok("KataGo"),
            "version" => ok("1.16.0"),
            "protocol_version" => ok("2"),
            "list_commands" => ok(COMMANDS),
            "kata-get-rules" => ok(RULES),
            "boardsize" | "rectangular_boardsize" | "clear_board" => {
                self.stones = 0;
                self.moves.clear();
                ok("")
            }
            "set_position" => {
                self.stones = args.len() / 2;
                self.moves.clear();
                ok("")
            }
            "undo" => {
                self.moves.pop();
                ok("")
            }
            "play" => {
                let mv = args.join(" ");
                if self.refuse.contains(&mv) {
                    format!("?{id} illegal move\n\n")
                } else {
                    self.moves.push(mv);
                    ok("")
                }
            }
            "kata-analyze" => {
                self.analysing = true;
                let position = self.stones + self.moves.len();
                if command.contains("maxmoves 1") {
                    self.searches.push((id, position));
                }
                let v = code(position);
                let visits = if command.contains("maxmoves 1") { 1000 + id } else { 1 };
                format!(
                    "={id}\ninfo move {v} visits {visits} edgeVisits {visits} utility 0.1 winrate 0.6 scoreMean 1.5 \
                     scoreStdev 9.0 scoreLead 1.5 scoreSelfplay 1.5 prior 0.4 lcb 0.58 utilityLcb 0.1 weight {visits} \
                     order 0 pv {v} rootInfo visits {visits} winrate 0.6 scoreLead 1.5 scoreSelfplay 1.5 utility 0.1\n"
                )
            }
            "stop" if self.refuse.iter().any(|r| r == "stop") => {
                let at = self.refuse.iter().position(|r| r == "stop").unwrap();
                self.refuse.remove(at);
                format!("?{id} cannot stop\n\n")
            }
            _ => ok(""),
        }
    }

    fn emit(&mut self, text: Option<String>) {
        if let Some(text) = text {
            if self.held.is_empty() {
                self.put_out(text);
            } else {
                self.held.push_back(text);
            }
        }
    }

    fn hold(&mut self, text: Option<String>) {
        self.held.extend(text);
    }

    fn release(&mut self) {
        let held: Vec<String> = self.held.drain(..).collect();
        for text in held {
            self.put_out(text);
        }
        self.held_commands.clear();
    }

    fn put_out(&mut self, text: String) {
        self.written += text.matches('\n').count();
        self.out.push(text);
    }
}

/// The best move a search of a position with `n` stones and moves reports.
fn code(n: usize) -> String {
    const COLUMNS: &[u8] = b"ABCDEFGHJKLMNOPQRST";
    format!("{}{}", COLUMNS[n % 19] as char, (n / 19) % 19 + 1)
}

fn point(n: usize) -> Point {
    parse_vertex(BoardSize { width: 19, height: 19 }, &code(n))
        .unwrap()
        .unwrap()
}

/// A loopback 智子 service: Engine.IO open, the namespace connect, `ready`, then `stdin` events to
/// the engine and its output back as `stdout` events.
struct Zhizi {
    url: String,
    engine: Arc<Mutex<Engine>>,
}

fn zhizi(policy: impl FnMut(&str, bool) -> Answer + Send + 'static) -> Zhizi {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let engine = Arc::new(Mutex::new(Engine {
        stones: 0,
        moves: Vec::new(),
        analysing: false,
        log: Vec::new(),
        searches: Vec::new(),
        searched_unanswered: false,
        out: Vec::new(),
        written: 0,
        held: VecDeque::new(),
        held_commands: Vec::new(),
        until_next: false,
        policy: Box::new(policy),
        refuse: Vec::new(),
    }));
    let shared = engine.clone();
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            let engine = shared.clone();
            thread::spawn(move || serve(stream, &engine));
        }
    });
    Zhizi { url, engine }
}

fn serve(stream: TcpStream, engine: &Mutex<Engine>) {
    let Ok(mut ws) = accept(stream) else { return };
    let open = "0{\"sid\":\"fixture\",\"upgrades\":[],\"pingInterval\":25000,\"pingTimeout\":20000}";
    if ws.send(Message::Text(open.into())).is_err() {
        return;
    }
    loop {
        match ws.read() {
            Ok(Message::Text(text)) if text.starts_with("40") => break,
            Ok(_) => {}
            Err(_) => return,
        }
    }
    for text in ["40{\"sid\":\"fixture\"}", "42[\"ready\"]"] {
        if ws.send(Message::Text(text.into())).is_err() {
            return;
        }
    }
    ws.get_mut().set_read_timeout(Some(Duration::from_millis(5))).unwrap();
    loop {
        match ws.read() {
            Ok(Message::Text(text)) => {
                if let Some(event) = text.strip_prefix("42") {
                    let event: Value = serde_json::from_str(event).unwrap();
                    if event[0] == "stdin" {
                        let mut engine = engine.lock().unwrap();
                        for line in event[1].as_str().unwrap().lines().filter(|l| !l.trim().is_empty()) {
                            engine.command(line);
                        }
                    }
                }
            }
            Ok(Message::Close(_)) => return,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(_) => return,
        }
        let out = std::mem::take(&mut engine.lock().unwrap().out);
        for text in out {
            if ws
                .send(Message::Text(format!("42{}", json!(["stdout", text])).into()))
                .is_err()
            {
                return;
            }
        }
    }
}

impl Zhizi {
    fn log(&self) -> Vec<String> {
        self.engine.lock().unwrap().log.clone()
    }
    fn searches(&self) -> Vec<(u64, usize)> {
        self.engine.lock().unwrap().searches.clone()
    }
    fn held_commands(&self) -> Vec<String> {
        self.engine.lock().unwrap().held_commands.clone()
    }
    fn release(&self) {
        self.engine.lock().unwrap().release();
    }
    fn set_policy(&self, policy: impl FnMut(&str, bool) -> Answer + Send + 'static) {
        self.engine.lock().unwrap().policy = Box::new(policy);
    }
    fn refuse(&self, mv: &str) {
        self.engine.lock().unwrap().refuse.push(mv.into());
    }
    /// The setup stones and moves the engine holds.
    fn board(&self) -> (usize, Vec<String>) {
        let engine = self.engine.lock().unwrap();
        (engine.stones, engine.moves.clone())
    }
}

/// A workbench whose ready 智子 foreground engine analyses `record`.
fn foreground(z: &Zhizi, record: &str) -> Bench {
    let url = z.url.clone();
    let connector = CloudConnector::new(Arc::new(move |_, _| {
        Ok(SocketCredentials {
            socket_url: url.clone(),
            token: "fixture-token".into(),
        })
    }));
    let mut b = workbench::bench_with(connector);
    b.open(record);
    let settings = CloudSettings {
        provider: Provider::Zhizi,
        identifier: "fixture-account".into(),
        ..CloudSettings::default()
    };
    b.wb.activate_cloud(b.wb.cloud_ticket(), settings, true).unwrap();
    assert!(
        b.pump_until(Duration::from_secs(10), |wb| workbench::ready(wb) && pondering(z, 0)),
        "{:?}",
        z.log()
    );
    b
}

/// The foreground engine analyses (a search that is not a borrowed one) after the first `from`
/// commands.
fn pondering(z: &Zhizi, from: usize) -> bool {
    z.log()[from.min(z.log().len())..]
        .iter()
        .any(|c| c.contains("kata-analyze") && !overview_search(c))
}

/// A borrowed engine's finite search.
fn overview_search(command: &str) -> bool {
    command.contains("kata-analyze") && command.contains("maxmoves 1")
}

fn command_name(line: &str) -> &str {
    line.split_whitespace().nth(1).unwrap_or_default()
}

fn completed(wb: &mut Workbench) -> bool {
    matches!(wb.inspect().flash_outcome, Some(FlashOutcome::Completed { .. })) && wb.inspect().flash.is_none()
}

/// Feeds the workbench until neither side has anything on its way: the engine has read every line
/// the workbench wrote, and the workbench every line the engine put out. With every other answer
/// held back by the test, the workbench then writes nothing more until one is released.
fn quiet(b: &mut Bench, z: &Zhizi) {
    let engine = z.engine.clone();
    assert!(
        b.pump_until_quiet(Duration::from_secs(20), || {
            let e = engine.lock().unwrap();
            (e.log.len(), e.written)
        }),
        "{:?}",
        z.log()
    );
}

/// Each analysed main-trunk move: its move number, side to play, best move and visits.
fn results(wb: &mut Workbench, moves: usize) -> Vec<(u32, Stone, Option<Point>, u64)> {
    wb.navigate(Navigation::First);
    let mut found = Vec::new();
    for _ in 0..moves {
        wb.navigate(Navigation::Forward(1));
        let v = wb.inspect();
        if let Some(a) = v.analysis {
            found.push((v.position.move_number, a.to_play, a.candidates[0].point, a.visits));
        }
    }
    found
}

fn side(move_number: u32) -> Stone {
    if move_number % 2 == 1 {
        Stone::White
    } else {
        Stone::Black
    }
}

/// Every stored result is the search of its own position: the move the search of a board with
/// that many moves reports, the side to play there, and the visits of the latest such search.
fn assert_own_results(z: &Zhizi, found: &[(u32, Stone, Option<Point>, u64)]) {
    let searches = z.searches();
    for &(n, to_play, best, visits) in found {
        assert_eq!(best, Some(point(n as usize)), "move {n}: {found:?}");
        assert_eq!(to_play, side(n), "move {n}");
        let (id, _) = searches.iter().rev().find(|&&(_, p)| p == n as usize).unwrap();
        assert_eq!(visits, 1000 + id, "move {n} keeps the visits of its own search");
    }
}

const EIGHT: &str = "(;SZ[19];B[pd];W[dd];B[pp];W[dp];B[];W[qf];B[nc];W[fc])";
const FOUR: &str = "(;SZ[19];B[pd];W[dd];B[pp];W[dp])";

/// A normal run: the first position is rebuilt, then every next position is reached by its move
/// and searched in the same write, without waiting for the move's answer; only the last position
/// is stopped. Each result stays with its position, and the engine is given back analysing.
#[test]
fn moves_replace_stop_between_positions_and_each_result_stays_with_its_position() {
    // A move that ends an analysis is answered only once the next command came: a client that
    // waited for it before searching would never search.
    let z = zhizi(|command, analysing| {
        if analysing && command.starts_with("play ") {
            Answer::UntilNext
        } else {
            Answer::Now
        }
    });
    let mut b = foreground(&z, EIGHT);
    let from = z.log().len();
    assert_eq!(b.wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
    assert!(b.wb.inspect().flash.expect("a flash run").borrowed);
    assert!(
        b.pump_until(Duration::from_secs(20), completed),
        "{:?} {:?}",
        b.wb.inspect().flash_outcome,
        z.log()
    );
    assert!(
        b.pump_until(Duration::from_secs(10), |_| pondering(&z, from + 1)),
        "given back analysing"
    );

    let run: Vec<String> = z.log()[from..].to_vec();
    let searches: Vec<usize> = (0..run.len()).filter(|&i| overview_search(&run[i])).collect();
    assert_eq!(searches.len(), 8, "{run:#?}");
    let rules = run.iter().position(|c| command_name(c) == "kata-get-rules").unwrap();
    let setup: Vec<&str> = run[rules + 1..searches[0]].iter().map(|c| command_name(c)).collect();
    assert_eq!(
        &setup[..3],
        ["boardsize", "clear_board", "komi"],
        "the first position is rebuilt: {setup:?}"
    );
    // (Giving the engine back afterwards is unchanged and may undo.)
    assert!(
        !run[..searches[7]].iter().any(|c| command_name(c) == "undo"),
        "{run:#?}"
    );
    for pair in searches.windows(2) {
        assert_eq!(
            pair[1] - pair[0],
            2,
            "one move then the search: {:?}",
            &run[pair[0]..=pair[1]]
        );
        assert_eq!(command_name(&run[pair[0] + 1]), "play");
    }
    assert_eq!(
        command_name(&run[searches[7] + 1]),
        "stop",
        "the last position is stopped"
    );
    for &i in &searches {
        let interval: u32 = run[i].split_whitespace().nth(3).unwrap().parse().unwrap();
        assert!(interval <= 10, "report interval within 10 centiseconds: {}", run[i]);
    }

    let found = results(&mut b.wb, 8);
    assert_eq!(found.len(), 8, "{found:?}");
    assert_own_results(&z, &found);
    assert_eq!(b.wb.inspect().engine.unrestored, None);
    b.finish();
}

/// The finished position is published only once the end of its analysis and the answer to the
/// move that ended it both arrived, whichever comes first.
#[test]
fn a_finished_position_is_published_only_after_both_of_its_boundaries() {
    for order in [Answer::EndThenHeld, Answer::AnswerThenHeld] {
        let z = zhizi(move |command, analysing| {
            if analysing && command.starts_with("play ") {
                order
            } else {
                Answer::Now
            }
        });
        let mut b = foreground(&z, FOUR);
        assert_eq!(b.wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
        for finished in 1..=3u32 {
            assert!(
                b.pump_until(Duration::from_secs(10), |_| !z.held_commands().is_empty()),
                "{order:?}: {:?}",
                z.log()
            );
            // One boundary is in, the other held back: nothing for the finished position yet.
            quiet(&mut b, &z);
            let published = |wb: &mut Workbench| wb.inspect().history.iter().any(|h| h.move_number == finished);
            assert!(
                !published(&mut b.wb),
                "{order:?}: move {finished} published before both boundaries"
            );
            z.release();
            assert!(
                b.pump_until(Duration::from_secs(10), published),
                "{order:?}: move {finished}"
            );
        }
        assert!(
            b.pump_until(Duration::from_secs(10), completed),
            "{order:?}: {:?}",
            b.wb.inspect().flash_outcome
        );
        let found = results(&mut b.wb, 4);
        assert_eq!(found.len(), 4, "{order:?}");
        assert_own_results(&z, &found);
        b.finish();
    }
}

/// A position after setup stones needs the board rebuilt: the search before it is stopped, as is
/// the last position's.
#[test]
fn a_position_that_needs_a_rebuild_and_the_last_one_are_stopped() {
    let z = zhizi(|_, _| Answer::Now);
    let mut b = foreground(&z, "(;SZ[19];B[pd];W[dd];AB[pp]PL[W];W[qq])");
    let from = z.log().len();
    assert_eq!(b.wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
    assert!(b.pump_until(Duration::from_secs(20), completed), "{:?}", z.log());
    let run: Vec<String> = z.log()[from..].to_vec();
    let searches: Vec<usize> = (0..run.len()).filter(|&i| overview_search(&run[i])).collect();
    assert_eq!(searches.len(), 3, "{run:#?}");
    assert_eq!(
        command_name(&run[searches[0] + 1]),
        "play",
        "the second move follows the first"
    );
    let after_second: Vec<&str> = run[searches[1] + 1..searches[2]]
        .iter()
        .map(|c| command_name(c))
        .collect();
    assert_eq!(
        &after_second[..5],
        ["stop", "boardsize", "clear_board", "komi", "set_position"],
        "{after_second:?}"
    );
    assert_eq!(command_name(&run[searches[2] + 1]), "stop");
    assert_eq!(b.wb.inspect().history.iter().filter(|h| h.move_number > 0).count(), 3);
    b.finish();
}

/// The first position's moves are written up to eight unanswered, any other setup command alone,
/// and its search only once all of them are answered. (Other analyses, here a 局部精析 first,
/// keep stopping between positions.)
#[test]
fn the_first_position_replays_eight_moves_at_a_time_and_searches_once_all_are_answered() {
    let z = zhizi(|_, _| Answer::Now);
    let record = "(;SZ[19];B[pd];W[dd];B[pp];W[dp];B[qf];W[nc];B[fc];W[cf];B[jd];W[jp];B[dj];W[pj];B[jj];W[cn])";
    let mut b = foreground(&z, record);
    let from = z.log().len();
    let range = FlashTarget::MoveRange {
        start_move: 1,
        end_move: 13,
    };
    assert_eq!(b.wb.start_flash_analysis(range).unwrap(), Ok(true));
    assert!(b.pump_until(Duration::from_secs(20), completed), "{:?}", z.log());
    assert!(b.pump_until(Duration::from_secs(10), |_| pondering(&z, from + 1)));
    let range_run: Vec<String> = z.log()[from..].to_vec();
    let searches: Vec<usize> = (0..range_run.len())
        .filter(|&i| overview_search(&range_run[i]))
        .collect();
    assert_eq!(searches.len(), 12);
    for pair in searches.windows(2) {
        assert!(
            range_run[pair[0]..pair[1]].iter().any(|c| command_name(c) == "stop"),
            "局部精析 stops between positions"
        );
    }

    // The whole game asks only moves 13 and 14; every setup answer waits for the test.
    quiet(&mut b, &z);
    z.set_policy(|command, analysing| {
        let setup = [
            "boardsize",
            "clear_board",
            "komi",
            "set_position",
            "play",
            "kata-set-rules",
        ];
        if !analysing && setup.contains(&command.split_whitespace().next().unwrap()) {
            Answer::Held
        } else {
            Answer::Now
        }
    });
    let from = z.log().len();
    assert_eq!(b.wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
    // Each round is what the workbench writes with every answer held back.
    let mut rounds = Vec::new();
    loop {
        quiet(&mut b, &z);
        let held = z.held_commands();
        if held.is_empty() {
            break;
        }
        rounds.push(held);
        z.release();
    }
    let names: Vec<Vec<&str>> = rounds
        .iter()
        .map(|r| r.iter().map(|c| c.split_whitespace().next().unwrap()).collect())
        .collect();
    assert_eq!(
        names[..3],
        [vec!["boardsize"], vec!["clear_board"], vec!["komi"]],
        "{names:?}"
    );
    assert_eq!(names[3], vec!["play"; 8], "eight moves in flight: {names:?}");
    assert_eq!(names[4], vec!["play"; 5], "{names:?}");
    for round in &names[5..] {
        assert_eq!(round.len(), 1, "other setup goes alone: {names:?}");
    }
    assert!(
        !z.engine.lock().unwrap().searched_unanswered,
        "searched before the setup was answered"
    );
    z.set_policy(|_, _| Answer::Now);
    assert!(b.pump_until(Duration::from_secs(20), completed), "{:?}", z.log());
    let run: Vec<String> = z.log()[from..].to_vec();
    assert_eq!(run.iter().filter(|c| overview_search(c)).count(), 2, "{run:#?}");
    let found = results(&mut b.wb, 14);
    assert_eq!(found.len(), 14);
    assert_own_results(&z, &found[12..]);
    b.finish();
}

/// Cancelling while a finished position waits for its boundaries: nothing that arrives later is
/// stored anywhere, and the engine is given back and analyses again.
#[test]
fn cancelling_mid_run_stores_nothing_later_and_gives_the_engine_back() {
    let z = zhizi(|command, analysing| {
        if analysing && command.starts_with("play ") {
            Answer::EndThenHeld
        } else {
            Answer::Now
        }
    });
    let mut b = foreground(&z, FOUR);
    assert_eq!(b.wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
    assert!(
        b.pump_until(Duration::from_secs(10), |_| !z.held_commands().is_empty()),
        "{:?}",
        z.log()
    );
    let from = z.log().len();
    assert!(b.wb.cancel_flash_analysis());
    z.set_policy(|_, _| Answer::Now);
    z.release();
    assert!(
        b.pump_until(Duration::from_secs(10), |wb| !wb.inspect().engine.paused
            && pondering(&z, from)),
        "{:?}",
        z.log()
    );
    quiet(&mut b, &z);
    let v = b.wb.inspect();
    assert!(v.history.iter().all(|h| h.move_number == 0), "{:?}", v.history);
    assert!(v.flash.is_none() && v.flash_outcome.is_none());
    assert_eq!(v.engine.unrestored, None);
    assert!(z.log()[from..].iter().any(|c| command_name(c) == "name"), "given back");
    b.finish();
}

/// A move refused as illegal: the finished position keeps its result, the search written behind
/// the move is stored nowhere, the run fails as a failed request and the engine is given back.
#[test]
fn a_refused_move_keeps_the_finished_result_and_fails_the_run() {
    let z = zhizi(|_, _| Answer::Now);
    z.refuse("W D16");
    let mut b = foreground(&z, FOUR);
    let from = z.log().len();
    assert_eq!(b.wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
    assert!(
        b.pump_until(Duration::from_secs(10), |wb| wb.inspect().flash_outcome.is_some()),
        "{:?}",
        z.log()
    );
    let v = b.wb.inspect();
    assert_eq!(
        v.flash_outcome,
        Some(FlashOutcome::Failed {
            run: 1,
            message_key: "AnalysisEngine.requestDispatchFailed".into()
        })
    );
    let run: Vec<String> = z.log()[from..].to_vec();
    let searches: Vec<&String> = run.iter().filter(|c| overview_search(c)).collect();
    assert_eq!(
        searches.len(),
        2,
        "the search behind the refused move was written: {run:#?}"
    );
    let first: u64 = searches[0].split_whitespace().next().unwrap().parse().unwrap();
    let behind: u64 = searches[1].split_whitespace().next().unwrap().parse().unwrap();
    let points: Vec<(u32, u64)> = v
        .history
        .iter()
        .filter(|h| h.move_number > 0)
        .map(|h| (h.move_number, h.visits))
        .collect();
    assert_eq!(
        points,
        [(1, 1000 + first)],
        "only the finished position, with its own search"
    );
    assert!(
        b.pump_until(Duration::from_secs(10), |_| pondering(&z, from + 1)),
        "given back analysing"
    );
    assert_eq!(b.wb.inspect().engine.unrestored, None);
    let found = results(&mut b.wb, 4);
    assert!(
        found.iter().all(|&(_, _, _, visits)| visits != 1000 + behind),
        "{found:?}"
    );
    b.finish();
}

/// Budgets over 500 visits keep the lease's ordinary searches: a `stop` between positions.
#[test]
fn a_budget_over_five_hundred_visits_keeps_stopping_between_positions() {
    let z = zhizi(|_, _| Answer::Now);
    let mut b = foreground(&z, FOUR);
    let mut settings = b.wb.config().analysis_engine.clone();
    settings.max_visits = 500;
    let shown = settings.command.clone();
    assert_eq!(
        b.wb.save_analysis_engine_settings(settings, &shown, false).unwrap(),
        Ok(true)
    );
    let from = z.log().len();
    assert_eq!(b.wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
    assert!(b.pump_until(Duration::from_secs(20), completed), "{:?}", z.log());
    let run: Vec<String> = z.log()[from..].to_vec();
    let searches: Vec<usize> = (0..run.len()).filter(|&i| overview_search(&run[i])).collect();
    assert_eq!(searches.len(), 4);
    for pair in searches.windows(2) {
        assert_eq!(command_name(&run[pair[0] + 1]), "stop", "{run:#?}");
    }
    b.finish();
}

/// Moves 1–11 with 局部精析 having analysed moves 2–10: 整盘速览 then searches move 1 and goes on
/// to move 11, ten moves ahead. The engine is given back and analyses again.
fn moves_2_to_10_analysed(z: &Zhizi) -> Bench {
    let record = "(;SZ[19];B[pd];W[dd];B[pp];W[dp];B[qf];W[nc];B[fc];W[cf];B[jd];W[jp];B[dj])";
    let mut b = foreground(z, record);
    let from = z.log().len();
    let range = FlashTarget::MoveRange {
        start_move: 2,
        end_move: 11,
    };
    assert_eq!(b.wb.start_flash_analysis(range).unwrap(), Ok(true));
    assert!(b.pump_until(Duration::from_secs(20), completed), "{:?}", z.log());
    assert!(b.pump_until(Duration::from_secs(10), |_| pondering(z, from + 1)));
    b
}

/// A position more than eight moves ahead (局部精析 already analysed the ones between): the first
/// move still replaces `stop`; the rest is replayed at most eight unanswered moves at a time, and
/// the position is searched once all of them are answered.
#[test]
fn a_position_more_than_eight_moves_ahead_is_reached_by_its_moves_eight_at_a_time() {
    let z = zhizi(|_, _| Answer::Now);
    let mut b = moves_2_to_10_analysed(&z);

    // Between the first and the second search every move is held until the test releases it.
    let mut searched = 0;
    z.set_policy(move |command, _| {
        searched += usize::from(overview_search(command));
        if searched == 1 && command.starts_with("play ") {
            Answer::Held
        } else {
            Answer::Now
        }
    });
    let from = z.log().len();
    let before = z.searches().len();
    assert_eq!(b.wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
    // The move replacing `stop`, then the replay: eight unanswered moves, then the last one.
    let mut rounds = Vec::new();
    for batch in [1, 8, 1] {
        assert!(
            b.pump_until(Duration::from_secs(20), |_| z.held_commands().len() >= batch),
            "{rounds:?} {:#?}",
            z.log()
        );
        rounds.push(z.held_commands());
        z.release();
    }
    assert!(b.pump_until(Duration::from_secs(20), completed), "{:#?}", z.log());
    let positions: Vec<usize> = z.searches()[before..].iter().map(|&(_, p)| p).collect();
    assert_eq!(positions, [1, 11], "only moves 1 and 11 lack a result");
    let run: Vec<String> = z.log()[from..].to_vec();
    let searches: Vec<usize> = (0..run.len()).filter(|&i| overview_search(&run[i])).collect();
    let between: Vec<&str> = run[searches[0] + 1..searches[1]]
        .iter()
        .map(|c| c.split_once(' ').unwrap().1)
        .collect();
    assert_eq!(between[0], "play W D16", "move 2 ends the search of move 1: {run:#?}");
    assert_eq!(between.len(), 10, "moves 2–11, no `stop` or rebuild: {run:#?}");
    assert!(between.iter().all(|c| c.starts_with("play ")), "{run:#?}");
    let sizes: Vec<usize> = rounds.iter().map(Vec::len).collect();
    assert_eq!(sizes, [1, 8, 1], "the first move alone, then at most eight unanswered: {rounds:?}");
    assert!(
        !z.engine.lock().unwrap().searched_unanswered,
        "searched before the replay was answered"
    );
    let found = results(&mut b.wb, 11);
    assert_eq!(found.len(), 11);
    assert_own_results(&z, &[found[0], found[10]]);
    b.finish();
}

/// Cancelling while the first position's rebuild is in flight (`clear_board` done, its answer not
/// yet read, the move after it not written): the engine is given back holding the record's
/// position again, not the empty board the cancelled rebuild left.
#[test]
fn cancelling_during_the_first_rebuild_gives_the_engine_back_on_its_position() {
    let z = zhizi(|_, _| Answer::Now);
    let mut b = foreground(&z, "(;SZ[19]KM[7.5];B[pd])");
    b.wb.navigate(Navigation::Forward(1));
    assert!(
        b.pump_until(Duration::from_secs(10), |_| z.board() == (0, vec!["B Q16".to_string()])
            && pondering(&z, 0)),
        "{:?}",
        z.log()
    );
    let mut held = false;
    z.set_policy(move |command, _| {
        if !held && command == "clear_board" {
            held = true;
            Answer::Held
        } else {
            Answer::Now
        }
    });
    assert_eq!(b.wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
    assert!(b.wb.inspect().flash.is_some_and(|f| f.borrowed));
    assert!(
        b.pump_until(Duration::from_secs(10), |_| !z.held_commands().is_empty()),
        "{:?}",
        z.log()
    );
    assert_eq!(z.board(), (0, Vec::new()), "the engine cleared its board");
    assert!(b.wb.cancel_flash_analysis());
    let from = z.log().len();
    z.release();
    assert!(
        b.pump_until(Duration::from_secs(10), |_| pondering(&z, from)),
        "{:?}",
        z.log()
    );
    assert_eq!(z.board(), (0, vec!["B Q16".to_string()]), "{:?}", &z.log()[from..]);
    assert_eq!(b.wb.inspect().engine.unrestored, None);
    b.finish();
}

/// The visits the history shows for main-trunk move `n` (0: none).
fn visits_on(wb: &mut Workbench, n: u32) -> u64 {
    wb.inspect().history.iter().filter(|h| h.move_number == n).map(|h| h.visits).max().unwrap_or(0)
}

/// The whole-game run, the second flash run after 局部精析, is over.
fn second_run_over(wb: &mut Workbench) -> bool {
    let v = wb.inspect();
    v.flash.is_none() && matches!(v.flash_outcome, Some(FlashOutcome::Completed { run: 2 } | FlashOutcome::Failed { run: 2, .. }))
}

/// After the run the engine was given back: `name` confirmed it, then it analyses again.
fn given_back(b: &mut Bench, z: &Zhizi) {
    let named = z.log().iter().rposition(|c| command_name(c) == "name").unwrap();
    assert!(b.pump_until(Duration::from_secs(10), |_| pondering(z, named + 1)), "{:#?}", z.log());
    assert_eq!(b.wb.inspect().engine.unrestored, None);
}

/// The move to a position ten moves ahead is not answered within the boundary timeout: the `stop`
/// written then ends the finished search whether it is answered, refused or never answered. The
/// run fails without publishing move 1 or searching move 11, and the engine is given back.
#[test]
fn an_advance_timing_out_before_its_replay_fails_however_its_stop_is_answered() {
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Stop {
        Answered,
        Refused,
        NeverAnswered,
    }
    for stop in [Stop::Answered, Stop::Refused, Stop::NeverAnswered] {
        let z = zhizi(|_, _| Answer::Now);
        let mut b = moves_2_to_10_analysed(&z);
        // The finished search ends, the move's answer and everything after it are held.
        let (mut searched, mut held) = (0, false);
        z.set_policy(move |command, _| {
            searched += usize::from(overview_search(command));
            if searched == 1 && !held && command.starts_with("play ") {
                held = true;
                Answer::EndThenHeld
            } else {
                Answer::Now
            }
        });
        let from = z.log().len();
        let before = z.searches().len();
        assert_eq!(b.wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
        assert!(
            b.pump_until(Duration::from_secs(20), |_| z.held_commands() == ["play W D16"]),
            "{stop:?} {:#?}",
            &z.log()[from..]
        );
        // The lease's own `stop` was answered; the one written at the timeout is refused.
        if stop == Stop::Refused {
            z.refuse("stop");
        }
        let moved = z.log().len();
        let advanced = Instant::now();
        b.wb.engine_tick(advanced + Duration::from_millis(1200));
        assert!(
            b.pump_until(Duration::from_secs(10), |_| z.log()[moved..].iter().any(|c| command_name(c) == "stop")),
            "{stop:?}: the timeout writes `stop` {:#?}",
            &z.log()[from..]
        );
        if stop == Stop::NeverAnswered {
            b.wb.engine_tick(advanced + Duration::from_millis(2400));
        }
        z.release();
        assert!(b.pump_until(Duration::from_secs(20), second_run_over), "{stop:?} {:#?}", &z.log()[from..]);
        assert_eq!(
            b.wb.inspect().flash_outcome,
            Some(FlashOutcome::Failed {
                run: 2,
                message_key: "AnalysisEngine.requestDispatchFailed".into()
            }),
            "{stop:?}"
        );
        let positions: Vec<usize> = z.searches()[before..].iter().map(|&(_, p)| p).collect();
        assert_eq!(positions, [1], "{stop:?}: move 11 is not searched");
        assert!(visits_on(&mut b.wb, 1) < 1000, "{stop:?}: move 1 is not published");
        given_back(&mut b, &z);
        b.finish();
    }
}

/// The move to a position ten moves ahead is refused: move 1 keeps its result, nothing is written
/// for move 11, and the run fails once the engine is given back.
#[test]
fn a_refused_move_to_a_position_more_than_eight_moves_ahead_writes_nothing_more() {
    let z = zhizi(|_, _| Answer::Now);
    let mut b = moves_2_to_10_analysed(&z);
    z.refuse("W D16");
    let from = z.log().len();
    let before = z.searches().len();
    assert_eq!(b.wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
    assert!(b.pump_until(Duration::from_secs(20), second_run_over), "{:#?}", &z.log()[from..]);
    let run: Vec<String> = z.log()[from..].to_vec();
    let refused = run.iter().position(|c| c.ends_with(" play W D16")).unwrap();
    assert!(overview_search(&run[refused - 1]), "the move replaced `stop`: {run:#?}");
    assert_eq!(command_name(&run[refused + 1]), "stop", "the engine is given back next: {run:#?}");
    assert_eq!(
        b.wb.inspect().flash_outcome,
        Some(FlashOutcome::Failed {
            run: 2,
            message_key: "AnalysisEngine.requestDispatchFailed".into()
        })
    );
    let searches = &z.searches()[before..];
    assert_eq!(searches.len(), 1, "only move 1 is searched");
    assert_eq!(visits_on(&mut b.wb, 1), 1000 + searches[0].0, "move 1 keeps its result");
    given_back(&mut b, &z);
    b.finish();
}

//! 弈客 Socket.IO room following (`OnlineDialog.req2`, types 1 and 5) through the session's
//! public API: protocol order, heartbeat, the room's game, restarts, reconnects and stops.

use std::time::Duration;

use lizzie_core::online::rtgame::{socket_url, Session};
use lizzie_core::online::{Connect, Frame, OnlineText, SourceUpdate, Step};
use lizzie_core::record::{GameRecord, NodeKind, Point, SourceAction, Stone, ROOT};
use serde_json::{json, Value};

const HANDSHAKE: &str = r#"0{"sid":"eio","upgrades":[],"pingInterval":25000,"pingTimeout":20000,"maxPayload":1000000}"#;
const ROOM: u64 = 123;
const INIT: &str = r#"42["init",{"game_info":{"sgf":"(;GM[1]SZ[19]KM[7.5];B[dd];W[pp](;B[dp])(;B[pd]))","blackName":"甲","whiteName":"乙","komi":6.5,"handicap":0,"boardSize":19,"status":1,"resultDesc":""}}]"#;

fn sent(step: &Step) -> Vec<String> {
    step.send
        .iter()
        .map(|f| match f {
            Frame::Text(t) => t.clone(),
            Frame::Binary(_) => panic!("binary frame"),
        })
        .collect()
}

/// `42<id>[event, data]`: the ack id and the arguments.
fn emitted(frame: &str) -> (u64, Value) {
    let rest = frame.strip_prefix("42").expect("event packet");
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let id = rest[..digits].parse().expect("ack id");
    (id, serde_json::from_str(&rest[digits..]).expect("json"))
}

fn only_emit(step: &Step) -> (u64, Value) {
    let frames = sent(step);
    assert_eq!(frames.len(), 1, "{frames:?}");
    emitted(&frames[0])
}

/// Opens the socket and runs the join chain; returns the session with nothing pending.
fn joined() -> Session {
    let (mut s, _) = Session::new(ROOM);
    s.on_open();
    s.on_text(HANDSHAKE);
    s.on_text(r#"40{"sid":"sio"}"#);
    s.on_text(r#"42["userinfo",{"user_id":77}]"#);
    s.on_text(r#"431[{"code":0}]"#);
    s.on_text(r#"432[{"code":0}]"#);
    s.on_text(r#"433[{"code":0,"data":[]}]"#);
    s
}

fn game(step: &Step) -> &GameRecord {
    step.updates
        .iter()
        .find_map(|u| match u {
            SourceUpdate::Game { record, .. } => Some(record.as_ref()),
            _ => None,
        })
        .expect("a Game update")
}

fn main_line(rec: &GameRecord) -> Vec<NodeKind> {
    let mut out = Vec::new();
    let mut node = ROOT;
    while let Some(next) = rec.main_child(node) {
        if rec.node(next).kind.is_action() {
            out.push(rec.node(next).kind);
        }
        node = next;
    }
    out
}

fn mv(color: Stone, x: u16, y: u16) -> NodeKind {
    NodeKind::Move { color, point: Point { x, y } }
}

fn after(step: &Step) -> Duration {
    step.connect.as_ref().expect("a reconnect").after
}

#[test]
fn join_runs_login_userinfo_entry_channel_listen_in_order() {
    let (mut s, step) = Session::new(ROOM);
    assert_eq!(step.connect, Some(Connect { url: socket_url(), after: Duration::ZERO }));
    assert!(!step.close);
    assert!(sent(&s.on_open()).is_empty());
    assert_eq!(sent(&s.on_text(HANDSHAKE)), ["40"]);

    let (id, login) = only_emit(&s.on_text(r#"40{"sid":"sio"}"#));
    assert_eq!(id, 0);
    assert_eq!(
        login,
        json!(["login", {"hall": "1", "room": ROOM, "token": -1, "user_id": -1_000_000, "platform": 3}])
    );
    // The login ack continues nothing; the server's userinfo does.
    assert!(sent(&s.on_text(r#"430[{"code":0}]"#)).is_empty());

    let (id, entry) = only_emit(&s.on_text(r#"42["userinfo",{"user_id":77}]"#));
    assert_eq!(id, 1);
    assert_eq!(entry, json!(["entry_room", {"hall": "1", "room": ROOM, "platform": 3, "user_id": 77}]));

    let (id, channel) = only_emit(&s.on_text(r#"431[{"code":0,"data":{}}]"#));
    assert_eq!(id, 2);
    assert_eq!(
        channel,
        json!(["channel/add", {"hall": "1", "room": ROOM, "platform": 3, "channel": "chat_1_123"}])
    );

    let (id, listen) = only_emit(&s.on_text(r#"432[{"code":0}]"#));
    assert_eq!(id, 3);
    assert_eq!(listen, json!(["comment/listen", {"hall": "1", "room": ROOM, "platform": 3}]));

    let done = s.on_text(r#"433[{"code":0,"data":[]}]"#);
    assert!(sent(&done).is_empty() && done.updates.is_empty());
}

#[test]
fn ping_is_answered_and_a_missed_ping_reconnects() {
    let mut s = joined();
    assert_eq!(sent(&s.on_text("2")), ["3"]);
    let due = s.next_due().expect("heartbeat deadline");
    let early = s.tick(due - Duration::from_secs(1));
    assert!(!early.close && early.connect.is_none());
    let late = s.tick(due);
    assert!(late.close);
    let wait = after(&late);
    assert!((Duration::from_millis(1000)..=Duration::from_millis(1500)).contains(&wait), "{wait:?}");
}

#[test]
fn init_reports_the_main_line_players_komi_and_handicap() {
    let mut s = joined();
    let step = s.on_text(INIT);
    assert_eq!(step.updates.len(), 1);
    let SourceUpdate::Game { record, names, komi, handicap } = &step.updates[0] else { panic!("Game") };
    assert_eq!(
        main_line(record),
        [mv(Stone::Black, 3, 3), mv(Stone::White, 15, 15), mv(Stone::Black, 3, 15)]
    );
    // The commentary variation of the SGF is not part of the source game.
    assert_eq!(record.nodes().count(), 4);
    assert_eq!(names.as_ref(), Some(&("甲".to_string(), "乙".to_string())));
    assert_eq!(*komi, Some(6.5));
    assert_eq!(*handicap, Some(0));
}

#[test]
fn init_takes_the_board_size_of_the_game_info() {
    let mut s = joined();
    let step = s.on_text(r#"42["init",{"game_info":{"sgf":"(;GM[1];B[cc])","boardSize":13}}]"#);
    let rec = game(&step);
    assert_eq!((rec.size.width, rec.size.height), (13, 13));
    assert_eq!(main_line(rec), [mv(Stone::Black, 2, 2)]);
}

#[test]
fn moves_append_truncate_and_pass_like_java() {
    let mut s = joined();
    s.on_text(INIT);

    let step = s.on_text(r#"42["move",{"move":{"mcnt":4,"x":15,"y":3,"c":-1}}]"#);
    assert_eq!(
        main_line(game(&step)),
        [mv(Stone::Black, 3, 3), mv(Stone::White, 15, 15), mv(Stone::Black, 3, 15), mv(Stone::White, 15, 3)]
    );

    // The same move again changes nothing.
    assert!(s.on_text(r#"42["move",{"move":{"mcnt":4,"x":15,"y":3,"c":-1}}]"#).updates.is_empty());

    // A move numbered 3 takes back moves 3 and 4 and plays in their place.
    let step = s.on_text(r#"42["move",{"move":{"mcnt":3,"x":2,"y":2,"c":1}}]"#);
    assert_eq!(
        main_line(game(&step)),
        [mv(Stone::Black, 3, 3), mv(Stone::White, 15, 15), mv(Stone::Black, 2, 2)]
    );

    // A point off the board is a pass.
    let step = s.on_text(r#"42["move",{"move":{"mcnt":4,"x":-1,"y":-1,"c":-1}}]"#);
    let line = main_line(game(&step));
    assert_eq!(line.len(), 4);
    assert_eq!(line[3], NodeKind::Pass { color: Stone::White });
}

#[test]
fn moves_before_init_are_ignored() {
    let mut s = joined();
    assert!(s.on_text(r#"42["move",{"move":{"mcnt":1,"x":3,"y":3,"c":1}}]"#).updates.is_empty());
}

#[test]
fn move_delete_restarts_on_a_new_socket_with_a_fresh_game() {
    let mut s = joined();
    s.on_text(INIT);
    let step = s.on_text(r#"42["move_delete",{}]"#);
    assert!(step.close);
    assert_eq!(step.connect, Some(Connect { url: socket_url(), after: Duration::ZERO }));
    // The game is gone until the next init.
    assert!(s.on_text(r#"42["move",{"move":{"mcnt":4,"x":15,"y":3,"c":-1}}]"#).updates.is_empty());

    s.on_open();
    assert_eq!(sent(&s.on_text(HANDSHAKE)), ["40"]);
    let (id, login) = only_emit(&s.on_text(r#"40{"sid":"sio2"}"#));
    assert_eq!(id, 0, "a new socket counts acks from 0");
    assert_eq!(login[1]["user_id"], json!(77), "the user id stays");
    let step = s.on_text(r#"42["init",{"game_info":{"sgf":"(;GM[1];B[dd];W[pp])"}}]"#);
    assert_eq!(main_line(game(&step)), [mv(Stone::Black, 3, 3), mv(Stone::White, 15, 15)]);
}

#[test]
fn reconnect_backoff_grows_to_its_cap_and_resets_after_an_open() {
    let mut s = joined();
    // A connection that had opened: reset backoff, 1000 ms moved up by at most half.
    let step = s.on_closed(None);
    let wait = after(&step);
    assert!((Duration::from_millis(1000)..=Duration::from_millis(1500)).contains(&wait), "{wait:?}");

    // Attempts that fail before opening take the next steps: 2000, 4000, 8000 ms ± half,
    // always within 1000..=5000 ms.
    let bounds = [(1000, 3000), (2000, 5000), (4000, 5000), (5000, 5000), (5000, 5000)];
    for (low, high) in bounds {
        let step = s.on_closed(Some("refused"));
        assert!(step.connect.as_ref().is_some_and(|c| c.url == socket_url()));
        let wait = after(&step);
        assert!(
            (Duration::from_millis(low)..=Duration::from_millis(high)).contains(&wait),
            "{wait:?} not in {low}..={high}"
        );
    }

    // An attempt that never opens times out the same way.
    let due = s.next_due().expect("connect timeout");
    assert_eq!(after(&s.tick(due)), Duration::from_millis(5000));

    // A connection that opens resets the backoff.
    s.on_open();
    s.on_text(HANDSHAKE);
    let wait = after(&s.on_closed(None));
    assert!((Duration::from_millis(1000)..=Duration::from_millis(1500)).contains(&wait), "{wait:?}");
}

#[test]
fn ack_code_10_stops_without_reconnecting() {
    let (mut s, _) = Session::new(ROOM);
    s.on_open();
    s.on_text(HANDSHAKE);
    s.on_text(r#"40{"sid":"sio"}"#);
    s.on_text(r#"42["userinfo",{"user_id":77}]"#);
    let step = s.on_text(r#"431[{"code":10}]"#);
    assert!(step.close && step.connect.is_none() && sent(&step).is_empty());
    let stopped = OnlineText::with("Online.text", ["已停止同步".to_string()]);
    assert!(matches!(&step.updates[..], [SourceUpdate::Stopped(t)] if *t == stopped));
    assert_eq!(s.next_due(), None);
    assert!(s.on_closed(None).connect.is_none());
}

#[test]
fn ack_codes_1_and_2_report_the_message() {
    let (mut s, _) = Session::new(ROOM);
    s.on_open();
    s.on_text(HANDSHAKE);
    s.on_text(r#"40{"sid":"sio"}"#);
    s.on_text(r#"42["userinfo",{"user_id":77}]"#);
    let step = s.on_text(r#"431[{"code":1,"message":"房间不存在"}]"#);
    let expected = OnlineText::with("YikeLiveDialog.syncFailed", ["房间不存在".to_string()]);
    assert!(matches!(&step.updates[..], [SourceUpdate::Status(t)] if *t == expected));
    assert!(!step.close);
}

#[test]
fn update_game_finishes_and_closes_for_good() {
    let mut s = joined();
    s.on_text(INIT);
    let step = s.on_text(r#"42["update_game",{"status":3,"resultDesc":"黑中盘胜"}]"#);
    assert!(step.close && step.connect.is_none());
    assert!(matches!(
        &step.updates[..],
        [SourceUpdate::Finished { result, set_result: true }] if result == "黑中盘胜"
    ));
    assert_eq!(s.next_due(), None);
    assert!(s.on_closed(None).connect.is_none());
}

#[test]
fn a_finished_init_reports_the_game_then_the_result() {
    let mut s = joined();
    let step =
        s.on_text(r#"42["init",{"game_info":{"sgf":"(;GM[1];B[dd])","status":3,"resultDesc":"白中盘胜"}}]"#);
    assert!(matches!(&step.updates[0], SourceUpdate::Game { .. }));
    assert!(matches!(
        &step.updates[1],
        SourceUpdate::Finished { result, set_result: true } if result == "白中盘胜"
    ));
    assert!(!step.close);
}

#[test]
fn comments_and_commentator_branches() {
    let mut s = joined();
    s.on_text(INIT);
    let comment = r#"42["comments",{"type":"add","content":{"type":2,"handsCount":12,"id":5,"content":"好棋","extend":"{\"LiveMember\":\"柯洁\"}"}}]"#;
    let step = s.on_text(comment);
    assert!(matches!(
        &step.updates[..],
        [SourceUpdate::Comment { move_number: 12, text }] if text == "柯洁：好棋"
    ));
    // The same comment delivered again is not appended twice.
    assert!(s.on_text(comment).updates.is_empty());
    // An updated comment names the line it replaces.
    let step = s.on_text(
        r#"42["comments",{"type":"update","content":{"type":2,"handsCount":12,"id":5,"content":"妙手","extend":"{\"LiveMember\":\"柯洁\"}"}}]"#,
    );
    assert!(matches!(
        &step.updates[..],
        [SourceUpdate::CommentUpdate { move_number: 12, old, text }] if old == "柯洁：好棋" && text == "柯洁：妙手"
    ));

    let step = s.on_text(
        r#"42["comments",{"type":"add","content":{"type":2,"handsCount":3,"id":6,"content":"开局","extend":"{}"}}]"#,
    );
    assert!(matches!(
        &step.updates[..],
        [SourceUpdate::Comment { move_number: 3, text }] if text == "开局"
    ));

    // The listen ack replays the room's branches (type 1: SGF nodes read from that move, a
    // nested game tree is not read) and comments (type 2).
    let (mut s, _) = Session::new(ROOM);
    s.on_open();
    s.on_text(HANDSHAKE);
    s.on_text(r#"40{"sid":"sio"}"#);
    s.on_text(r#"42["userinfo",{"user_id":77}]"#);
    s.on_text(r#"431[{"code":0}]"#);
    s.on_text(r#"432[{"code":0}]"#);
    s.on_text(INIT);
    let step = s.on_text(
        r#"433[{"code":0,"data":[{"type":1,"handsCount":2,"id":9,"content":";B[aa]C[看];W[]"},{"type":1,"handsCount":2,"id":11,"content":"(;B[cc];W[dd])"},{"type":2,"handsCount":2,"id":10,"content":"变化","extend":"{\"LiveMember\":\"解说\"}"}]}]"#,
    );
    let expected = vec![
        SourceAction { kind: NodeKind::Move { color: Stone::Black, point: Point { x: 0, y: 0 } }, comment: "看".into() },
        SourceAction { kind: NodeKind::Pass { color: Stone::White }, comment: String::new() },
    ];
    assert!(matches!(
        &step.updates[..],
        [
            SourceUpdate::Branch { move_number: 2, actions },
            SourceUpdate::Comment { move_number: 2, text },
        ] if *actions == expected && text == "解说：变化"
    ), "{:?}", step.updates);
}

#[test]
fn an_init_without_sgf_leaves_the_board_and_closes() {
    let mut s = joined();
    let step = s.on_text(r#"42["init",{"game_info":{"sgf":"","blackName":"甲","whiteName":"乙"}}]"#);
    assert!(step.close && step.connect.is_none());
    assert!(matches!(&step.updates[..], [SourceUpdate::NoGame { finished: false }]));

    let mut s = joined();
    let step = s.on_text(r#"42["init",{"game_info":{"sgf":"","resultDesc":"黑胜"}}]"#);
    assert!(matches!(&step.updates[..], [SourceUpdate::NoGame { finished: true }]), "{:?}", step.updates);
}

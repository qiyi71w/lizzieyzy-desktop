//! 野狐分享 / 腾讯欢乐 broadcasts (`online::huanle::Session`), driven with frames built the way
//! Java's decoder reads them (`OnlineDialog.parseReq`, `Fragment.parseProto`, `decodeNN`).

use std::time::{Duration, Instant};

use lizzie_core::online::huanle::Session;
use lizzie_core::online::{Connect, Frame, HttpReply, OnlineText, SourceUpdate, Step};
use lizzie_core::record::{GameRecord, NodeId, NodeKind, Point, SourceAction, Stone, ROOT};

const REFRESH: Duration = Duration::from_secs(5);
const FOX_URL: &str = "wss://hlws.huanle.qq.com/wqbroadcastlotus";
const HALL_URL: &str = "ws://wshall.huanle.qq.com/NewEagleEyeLotus";
const CHESSID: &str = "383699091456898";

fn query(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn fox_query() -> Vec<(String, String)> {
    query(&[
        ("gameid", "369"),
        ("showtype", "1"),
        ("showid", "83"),
        ("chessid", CHESSID),
        ("status", "0"),
        ("createtime", "1559816204"),
    ])
}

/// A type 3 session with its socket open.
fn fox() -> Session {
    let (mut s, _) = Session::new(&fox_query(), None, REFRESH);
    s.on_open();
    s
}

// --- test-side encoder, the inverse of Java's decoder -------------------------------------

fn varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 127) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 128);
    }
}

fn int_field(out: &mut Vec<u8>, field: u64, v: u64) {
    varint(out, field << 3);
    varint(out, v);
}

fn bytes_field(out: &mut Vec<u8>, field: u64, bytes: &[u8]) {
    varint(out, (field << 3) | 2);
    varint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

/// `parseProto`: size (the bytes after it), type, one skipped byte, body, four trailing bytes.
fn frag(kind: u16, body: &[u8]) -> Vec<u8> {
    let mut f = Vec::new();
    f.extend_from_slice(&((body.len() + 7) as u16).to_be_bytes());
    f.extend_from_slice(&kind.to_be_bytes());
    f.push(0);
    f.extend_from_slice(body);
    f.extend_from_slice(&[0; 4]);
    f
}

fn push_frags(out: &mut Vec<u8>, frags: &[Vec<u8>]) {
    out.extend_from_slice(&(frags.len() as i32).to_be_bytes());
    for f in frags {
        out.extend_from_slice(&(f.len() as i16).to_be_bytes());
        out.extend_from_slice(f);
    }
}

/// msg 23406, the reply to the broadcast request.
fn broadcast(fox: bool, result: i32, frags: &[Vec<u8>]) -> Vec<u8> {
    let mut m = vec![0, 0];
    m.extend_from_slice(&23406i16.to_be_bytes());
    m.extend_from_slice(&[0; 14]);
    m.extend_from_slice(&result.to_be_bytes());
    m.extend_from_slice(&[0; 16]);
    push_frags(&mut m, frags);
    m.extend_from_slice(&[0; 12]);
    m.extend_from_slice(&0i16.to_be_bytes());
    m.push(0);
    if fox {
        m.extend_from_slice(&[0; 12]);
    }
    m
}

/// msg 23407, pushed fragments.
fn push(frags: &[Vec<u8>]) -> Vec<u8> {
    let mut m = vec![0, 0];
    m.extend_from_slice(&23407i16.to_be_bytes());
    m.extend_from_slice(&[0; 30]);
    push_frags(&mut m, frags);
    m
}

fn player(name: &str, chinese: &str, region: u64) -> Vec<u8> {
    let mut p = Vec::new();
    bytes_field(&mut p, 1, name.as_bytes());
    bytes_field(&mut p, 2, chinese.as_bytes());
    int_field(&mut p, 4, region);
    p
}

/// 20032: AAA307 {4 handicap, 5 komi, 10 rule, 16 size}, AAA308 White, AAA309 Black.
fn new_game(size: u64, handicap: u64, komi: u64, rule: u64) -> Vec<u8> {
    let mut info = Vec::new();
    int_field(&mut info, 4, handicap);
    int_field(&mut info, 5, komi);
    int_field(&mut info, 10, rule);
    int_field(&mut info, 16, size);
    let mut body = Vec::new();
    int_field(&mut body, 1, 1);
    bytes_field(&mut body, 6, &info);
    bytes_field(&mut body, 7, &player("White Player", "白方", 1));
    bytes_field(&mut body, 8, &player("Black Player", "黑方", 86));
    frag(20032, &body)
}

/// 7005: AAA102 move number, AAA106 point, AAA158 colour.
fn mv(number: u64, color: Stone, x: u64, y: u64) -> Vec<u8> {
    let base = if color == Stone::Black { 1024 } else { 2048 };
    let mut body = Vec::new();
    int_field(&mut body, 4, base + y * 32 + x);
    int_field(&mut body, 6, if color == Stone::White { 1 } else { 2 });
    int_field(&mut body, 8, number);
    frag(7005, &body)
}

/// 7185: AAA166 winner, AAA167 margin in hundredths.
fn result(winner: u64, margin: u64) -> Vec<u8> {
    let mut body = Vec::new();
    int_field(&mut body, 4, winner);
    int_field(&mut body, 5, margin);
    frag(7185, &body)
}

// --- reading what the session reports ---------------------------------------------------

fn be16(f: &[u8], at: usize) -> i16 {
    i16::from_be_bytes([f[at], f[at + 1]])
}

fn be32(f: &[u8], at: usize) -> i32 {
    i32::from_be_bytes([f[at], f[at + 1], f[at + 2], f[at + 3]])
}

fn binary(step: &Step) -> &[u8] {
    match step.send.as_slice() {
        [Frame::Binary(f)] => f,
        other => panic!("expected one binary frame, got {other:?}"),
    }
}

fn last_record(step: &Step) -> &GameRecord {
    step.updates
        .iter()
        .rev()
        .find_map(|u| match u {
            SourceUpdate::Game { record, .. } => Some(record.as_ref()),
            _ => None,
        })
        .expect("a Game update")
}

fn main_line(rec: &GameRecord) -> Vec<NodeKind> {
    let mut line = Vec::new();
    let mut id: NodeId = ROOT;
    while let Some(next) = rec.main_child(id) {
        line.push(rec.node(next).kind);
        id = next;
    }
    line
}

fn stone(color: Stone, x: u16, y: u16) -> NodeKind {
    NodeKind::Move { color, point: Point { x, y } }
}

fn finished(step: &Step) -> Option<&str> {
    step.updates.iter().find_map(|u| match u {
        SourceUpdate::Finished { result, set_result } => {
            assert!(*set_result);
            Some(result.as_str())
        }
        _ => None,
    })
}

// --- tests ---------------------------------------------------------------------------------

#[test]
fn fox_share_connects_at_once_and_asks_for_the_broadcast_on_open() {
    let (mut s, step) = Session::new(&fox_query(), None, REFRESH);
    assert!(step.fetch.is_empty());
    assert_eq!(step.connect, Some(Connect { url: FOX_URL.into(), after: Duration::ZERO }));

    let step = s.on_open();
    let f = binary(&step);
    assert_eq!(f.len(), 90);
    assert_eq!(be16(f, 0), 90);
    assert_eq!(be32(f, 4), 1, "first sequence number");
    assert_eq!(be16(f, 16), 23406);
    assert_eq!(be32(f, 36), 369);
    assert_eq!(be32(f, 40), 1);
    assert_eq!(be32(f, 44), 83);
    assert_eq!(be32(f, 66), 1559816204);
    assert_eq!(s.next_due(), None, "no follow requests before the first reply");
}

#[test]
fn tencent_link_reads_the_request_first_then_connects_the_hall() {
    let q = query(&[("gametag", "abc"), ("uin", "42")]);
    let url = "http://wshall.huanle.qq.com/wxnseed/Broadcast/RequestBroadcast?callback=jQuery1&gametag=abc&uin=42";
    let (mut s, step) = Session::new(&q, Some(url), REFRESH);
    assert!(step.connect.is_none());
    let [need] = step.fetch.as_slice() else { panic!("one fetch") };
    assert_eq!(need.url, url);

    let body = r#"jQuery1({"result":0,"ResultID":0,"ShowType":2,"ShowID":"77","CreateTime":1600000000})"#;
    let step = s.on_http(need.purpose, &HttpReply::Ok { status: 200, body: body.into() });
    assert_eq!(step.connect, Some(Connect { url: HALL_URL.into(), after: Duration::ZERO }));

    let f = binary(&s.on_open()).to_vec();
    assert_eq!(be16(&f, 16), 23406);
    assert_eq!(be32(&f, 36), 369, "req0 sets gameid 369");
    assert_eq!(be32(&f, 40), 2);
    assert_eq!(be32(&f, 44), 77);
    assert_eq!(be32(&f, 66), 1600000000);
}

#[test]
fn tencent_link_stops_on_a_reply_it_cannot_read() {
    let q = query(&[("gametag", "abc"), ("uin", "42")]);
    let (mut s, step) = Session::new(&q, Some("http://example.invalid/r"), REFRESH);
    let purpose = step.fetch[0].purpose;
    let step = s.on_http(purpose, &HttpReply::Ok { status: 200, body: "<html>busy</html>".into() });
    assert!(step.connect.is_none());
    assert!(matches!(step.updates.as_slice(), [SourceUpdate::Stopped(_)]));
}

#[test]
fn tencent_link_stops_on_a_failed_reply_even_when_its_body_reads() {
    let q = query(&[("gametag", "abc"), ("uin", "42")]);
    let (mut s, step) = Session::new(&q, Some("http://example.invalid/r"), REFRESH);
    let purpose = step.fetch[0].purpose;
    let body = r#"jQuery1({"result":0,"ResultID":0,"ShowType":2,"ShowID":"77","CreateTime":1600000000})"#;
    let step = s.on_http(purpose, &HttpReply::Ok { status: 503, body: body.into() });
    assert!(step.connect.is_none());
    let expected = OnlineText::with("Online.status.pullError", ["HTTP 503".to_string()]);
    assert!(matches!(step.updates.as_slice(), [SourceUpdate::Stopped(t)] if *t == expected), "{:?}", step.updates);
}

#[test]
fn follow_requests_start_one_second_after_the_first_reply_then_every_refresh() {
    let mut s = fox();
    let before = Instant::now();
    s.on_binary(&broadcast(true, 0, &[]));
    let after = Instant::now();
    let due = s.next_due().expect("follow task scheduled");
    assert!(due >= before + Duration::from_secs(1) && due <= after + Duration::from_secs(1));

    assert!(s.tick(due - Duration::from_millis(1)).send.is_empty());
    let step = s.tick(due);
    let f = binary(&step);
    assert_eq!(f.len(), 54);
    assert_eq!(be16(f, 16), 23413);
    assert_eq!(be32(f, 4), 2, "sequence follows the broadcast request");
    assert_eq!(be32(f, 36), 369);
    assert_eq!(s.next_due(), Some(due + REFRESH));
}

#[test]
fn new_game_reports_size_komi_handicap_and_players() {
    let mut s = fox();
    // Type 3 keeps the Chinese rule: AAA10 = 1 doubles the komi (3.75 → 7.5).
    let step = s.on_binary(&broadcast(true, 0, &[new_game(19, 2, 375, 1)]));
    let Some(SourceUpdate::Game { record, names, komi, handicap }) = step.updates.first() else {
        panic!("expected a Game update, got {:?}", step.updates);
    };
    assert_eq!(record.size.width, 19);
    assert_eq!(names.as_ref().map(|(b, w)| (b.as_str(), w.as_str())), Some(("黑方", "White Player")));
    assert_eq!(*komi, Some(7.5));
    assert_eq!(*handicap, Some(2));
    let setup = &record.node(ROOT).setup.add_black;
    assert!(setup.contains(&Point { x: 15, y: 3 }) && setup.contains(&Point { x: 3, y: 15 }));
    assert!(main_line(record).is_empty());
}

#[test]
fn moves_append_and_a_resent_number_cuts_back() {
    let mut s = fox();
    s.on_binary(&broadcast(true, 0, &[new_game(19, 0, 0, 0)]));

    let step = s.on_binary(&push(&[mv(1, Stone::Black, 3, 3), mv(2, Stone::White, 15, 15)]));
    assert_eq!(main_line(last_record(&step)), vec![stone(Stone::Black, 3, 3), stone(Stone::White, 15, 15)]);

    let step = s.on_binary(&push(&[mv(3, Stone::Black, 15, 3)]));
    assert_eq!(main_line(last_record(&step)).len(), 3);

    // Move 2 again, elsewhere: what followed it goes and the new move takes its place.
    let step = s.on_binary(&push(&[mv(2, Stone::White, 16, 16)]));
    assert_eq!(main_line(last_record(&step)), vec![stone(Stone::Black, 3, 3), stone(Stone::White, 16, 16)]);
}

#[test]
fn a_commentator_variation_keeps_its_commentary_on_its_first_move() {
    let mut s = fox();
    s.on_binary(&broadcast(true, 0, &[new_game(19, 0, 0, 0)]));
    s.on_binary(&push(&[mv(1, Stone::Black, 3, 3), mv(2, Stone::White, 15, 15)]));
    // 8185 → AAA79: moves alternate from the side after move AAA20 − 1 = 2; AAA283 is the
    // commentary.
    let mut variation = Vec::new();
    int_field(&mut variation, 1, 1024 + 2 * 32 + 15);
    int_field(&mut variation, 1, 2048 + 15 * 32 + 2);
    int_field(&mut variation, 3, 3);
    bytes_field(&mut variation, 4, "妙手".as_bytes());
    let mut body = Vec::new();
    bytes_field(&mut body, 6, &variation);
    let step = s.on_binary(&push(&[frag(8185, &body)]));
    let expected = vec![
        SourceAction { kind: stone(Stone::Black, 15, 2), comment: "妙手\n".into() },
        SourceAction { kind: stone(Stone::White, 2, 15), comment: String::new() },
    ];
    assert!(
        matches!(&step.updates[..], [SourceUpdate::Branch { move_number: 2, actions }] if *actions == expected),
        "{:?}",
        step.updates
    );
}

#[test]
fn handicap_moves_are_numbered_after_the_stones() {
    let mut s = fox();
    s.on_binary(&broadcast(true, 0, &[new_game(19, 2, 50, 0)]));
    let step = s.on_binary(&push(&[mv(1, Stone::White, 2, 2), mv(2, Stone::Black, 16, 16)]));
    assert_eq!(main_line(last_record(&step)), vec![stone(Stone::White, 2, 2), stone(Stone::Black, 16, 16)]);
    let step = s.on_binary(&push(&[mv(2, Stone::Black, 16, 2)]));
    assert_eq!(main_line(last_record(&step)), vec![stone(Stone::White, 2, 2), stone(Stone::Black, 16, 2)]);
}

#[test]
fn result_closes_without_reconnecting() {
    let mut s = fox();
    s.on_binary(&broadcast(true, 0, &[new_game(19, 0, 650, 0)]));
    let step = s.on_binary(&push(&[mv(1, Stone::Black, 3, 3), result(2, 650)]));
    assert!(step.close);
    assert_eq!(finished(&step), Some("白胜6.5目"));
    assert_eq!(s.next_due(), None);
    let step = s.on_closed(None);
    assert!(step.connect.is_none());
    assert!(s.tick(Instant::now() + Duration::from_secs(3600)).send.is_empty());
}

#[test]
fn chinese_counting_reports_stones() {
    let mut s = fox();
    s.on_binary(&broadcast(true, 0, &[new_game(19, 0, 375, 1)]));
    let step = s.on_binary(&push(&[result(1, 375)]));
    assert_eq!(finished(&step), Some("黑胜3又3/4子"));
}

#[test]
fn a_close_before_the_end_reconnects() {
    let mut s = fox();
    s.on_binary(&broadcast(true, 0, &[new_game(19, 0, 650, 0)]));
    assert!(s.next_due().is_some());
    let step = s.on_closed(Some("connection reset"));
    assert_eq!(step.connect, Some(Connect { url: FOX_URL.into(), after: REFRESH }));
    assert_eq!(s.next_due(), None);

    let f = binary(&s.on_open()).to_vec();
    assert_eq!(be16(&f, 16), 23406);
    assert_eq!(be32(&f, 4), 1, "a new connection starts its sequence again");
}

#[test]
fn ended_broadcast_downloads_the_record() {
    let mut s = fox();
    let step = s.on_binary(&broadcast(true, 23409, &[new_game(19, 0, 650, 0)]));
    let [need] = step.fetch.as_slice() else { panic!("one record download") };
    assert_eq!(need.url, format!("https://weiqi.qq.com/openqipu/getqipu?callback=jQuery1&gamecode={CHESSID}"));
    assert_eq!(s.next_due(), None, "no follow requests once done");

    let sgf = "(;GM[1]FF[4]SZ[19]KM[6.5]PB[黑方]PW[白方]RE[B+R];B[pd];W[dd];B[pq])";
    let encoded: String = sgf
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect();
    let body = format!("jQuery1(\"{encoded}\",\"extra\")");
    let step = s.on_http(need.purpose, &HttpReply::Ok { status: 200, body });
    let rec = last_record(&step);
    assert_eq!(main_line(rec).len(), 3);
    assert_eq!(rec.info.black, "黑方");
    assert_eq!(finished(&step), None, "the record's RE is not written to the live end like 7185");

    assert!(s.on_closed(None).connect.is_none());
}

#[test]
fn a_failed_record_download_is_no_record_even_when_its_body_reads() {
    let mut s = fox();
    let step = s.on_binary(&broadcast(true, 23409, &[new_game(19, 0, 650, 0)]));
    let [need] = step.fetch.as_slice() else { panic!("one record download") };
    let body = "jQuery1(\"(;GM[1]FF[4]SZ[19];B[pd];W[dd])\",\"extra\")".to_string();
    let step = s.on_http(need.purpose, &HttpReply::Ok { status: 500, body });
    assert!(!step.updates.iter().any(|u| matches!(u, SourceUpdate::Game { .. })), "{:?}", step.updates);
    let expected = OnlineText::with("YikeLiveDialog.syncFailed", ["HTTP 500".to_string()]);
    assert!(matches!(step.updates.as_slice(), [SourceUpdate::Status(t)] if *t == expected), "{:?}", step.updates);
}

//! 在线棋谱 / 弈客直播 through the public surface: link classification, the 弈客 REST contract
//! (fixed Java `cb53acc` `YikeUrlParserTest`, `YikeSgfMainlineTest`, `YikeApiClientTest`), and a
//! followed live room driven through `Workbench::online` / `online_http` / `engine_tick` the way
//! the platform adapter does: the source owns only the main line, user branches and comments
//! stay, browsing follows only from the live end, stale replies are dropped.

use std::time::{Duration, Instant};

use lizzie_core::api::{CommandResult, EditCommand, LiveCategory, OnlineCommand, OnlineJob, PageReport};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::formats;
use lizzie_core::online::url::{classify, yike_room_key, OnlineSource};
use lizzie_core::online::{invalid_proxy_message, yike, HttpReply, OnlineText, SocketEvent};
use lizzie_core::record::{GameRecord, NodeId, NodeKind, Point, RecordFormat, ROOT};
use lizzie_core::settings::{NetworkSettings, ProxyMode, Storage};
use lizzie_core::workbench::{OpenOrigin, Workbench};

struct NoEngines;

impl Launcher for NoEngines {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        Err(std::io::Error::other("no engines in online tests"))
    }
}

const ROOM: &str = "https://home.yikeweiqi.com/#/live/new-room/186031/0/0";

// ------------------------------------------------------------------ links (YikeUrlParserTest)

#[test]
fn yike_links_map_to_the_java_sync_types() {
    assert_eq!(
        classify("https://home.yikeweiqi.com/#/live/room/18328/1/15630642"),
        Some(OnlineSource::YikeLiveRoom {
            id: "18328".into(),
            room: 15630642,
            detail: "https://api.yikeweiqi.com/golive/dtl?id=18328&flag=1".into()
        })
    );
    assert_eq!(
        classify("https://home.yikeweiqi.com/#/live/room/4903/0/0"),
        Some(OnlineSource::YikeLiveBoard {
            id: "4903".into(),
            url: "https://api.yikeweiqi.com/golive/dtl?id=4903".into()
        })
    );
    let new_room = classify(ROOM).unwrap();
    assert!(matches!(&new_room, OnlineSource::YikeNewLive { id, .. } if id == "186031"));
    assert_eq!(new_room.java_type(), 6);
    assert_eq!(
        classify("https://home.yikeweiqi.com/#/game/play/1/15630642"),
        Some(OnlineSource::YikeGameRoom {
            room: 15630642,
            detail: "https://api.yikeweiqi.com/golive/dtl?id=15630642".into()
        })
    );
    assert_eq!(
        classify("https://home.yikeweiqi.com/#/unite/66304678"),
        Some(OnlineSource::YikeUnite {
            room: 66304678,
            info_url: "https://game-server.yikeweiqi.com/game/info?id=66304678&sgf_option=true&players_option=true&setting_option=true&clock_option=true&is_void=true".into()
        })
    );
}

#[test]
fn lobby_pages_have_no_room_and_other_links_are_read_once() {
    for lobby in ["https://home.yikeweiqi.com/#/live", "https://home.yikeweiqi.com/#/game"] {
        assert_eq!(yike_room_key(lobby), None, "{lobby}");
        assert!(!classify(lobby).unwrap().is_yike(), "{lobby}");
    }
    assert_eq!(yike_room_key(ROOM).as_deref(), Some("live-room:186031"));
    assert_eq!(
        yike_room_key("https://home.yikeweiqi.com/#/unite/66304678").as_deref(),
        Some("unite-board:66304678")
    );
    assert_eq!(classify("   "), None);
    assert_eq!(
        classify("https://example.com/a.sgf"),
        Some(OnlineSource::Plain { url: "https://example.com/a.sgf".into() })
    );
    let fox = classify("https://h5.foxwq.com/share?gameid=12&createtime=34").unwrap();
    assert_eq!(fox.java_type(), 3);
    let tag = classify("https://h5.qq.com/x?gametag=ab&uin=9").unwrap();
    assert_eq!(tag.java_type(), 4);
}

/// The 弈客 kinds are recognized only on `https://yikeweiqi.com` and its subdomains (default
/// port, as the 弈客 web window loads). The same shapes elsewhere are any other link: read once
/// from the link itself, never from `api.<that host>`, and URL 无法识别 in the web window.
#[test]
fn yike_shaped_links_of_other_hosts_are_not_yike_rooms() {
    for yike in ["https://yikeweiqi.com/#/live/room/18328/1/15630642", "https://www.yikeweiqi.com/#/game/play/1/15630642"] {
        assert!(classify(yike).unwrap().is_yike(), "{yike}");
    }
    let shapes = [
        "/#/live/new-room/186031/0/0",
        "/#/live/room/18328/1/15630642",
        "/#/live/room/4903/0/0",
        "/#/live/room/4903",
        "/#/live/other/18328/1/15630642",
        "/#/live/other/4903",
        "/#/game/play/1/15630642",
        "/#/hall?room=15630642&hall=1",
        "/#/unite/66304678",
    ];
    let hosts = [
        "https://evil.example",
        "https://yikeweiqi.com.evil.example",
        "https://notyikeweiqi.com",
        "http://home.yikeweiqi.com",
        "https://home.yikeweiqi.com:8443",
    ];
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    for host in hosts {
        for shape in shapes {
            let link = format!("{host}{shape}");
            let plain = link.strip_suffix("/0/0").unwrap_or(&link).to_string();
            assert_eq!(classify(&link), Some(OnlineSource::Plain { url: plain.clone() }), "{link}");
            assert_eq!(yike_room_key(&link), None, "{link}");

            wb.online(OnlineCommand::Open { url: link.clone(), refresh_seconds: 1 }).unwrap();
            match wb.take_online_jobs().as_slice() {
                [OnlineJob::Get { url, .. }] => assert_eq!(url, &plain, "{link}"),
                other => panic!("{link}: {other:?}"),
            }
            wb.online(OnlineCommand::BrowserLoad { url: link.clone() }).unwrap();
            let o = wb.view(true).online;
            assert!(!o.browser_sync, "{link}");
            assert_eq!(o.browser_status.map(|s| s.args), Some(vec!["URL 无法识别".to_string()]), "{link}");
            assert!(wb.take_online_jobs().is_empty(), "{link}");
        }
    }
}

// ------------------------------------------------------------------ REST (YikeApiClientTest)

#[test]
fn request_signature_matches_the_web_client() {
    let headers = yike::headers(1000, 12345);
    let get = |k: &str| headers.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    assert_eq!(get("CurTime"), Some("1000"));
    assert_eq!(get("Nonce"), Some("12345"));
    assert_eq!(get("CheckSum"), Some("12d497b28b684dc903cccbe060271b17571d1d5d"));
    assert_eq!(get("accesstoken"), Some("15d1693d60abd643f7c680175a52b2b9"));
    assert_eq!(get("usertoken"), Some("-1"));
    assert_eq!(get("Platform"), Some("web"));
}

#[test]
fn live_list_rows_and_previews() {
    let body = r#"{"Status":1200,"Result":{"since":186031,"list":[
        {"Id":186031,"Version":2,"GameName":"LG Cup","BlackName":"Black Player","WhiteName":"White Player",
         "BlackCounty":"KR","WhiteCounty":"CN","Status":2,"FinishOrder":"2026-04-27","BroadcastTime":"20:07",
         "HandsCount":100,"PersonTimes":5325,"RealtimeAnalysisFlag":1,"BlackWinRate":67.8,"Delta":2.5,"hall":0,"room":0},
        {"Id":1,"Version":2,"GameName":"Upcoming","Status":1,"HandsCount":0,"hall":0,"room":0},
        {"Id":2,"Version":2,"GameName":"Starting","Status":2,"HandsCount":0,"hall":0,"room":0}]},"Message":""}"#;
    let (since, games) = yike::parse_list(body).unwrap();
    assert_eq!(since, 186031);
    let g = &games[0];
    assert_eq!((g.game.as_str(), g.hands, g.time.as_str()), ("LG Cup", 100, "2026-04-27 20:07"));
    assert_eq!((g.black.as_str(), g.white.as_str()), ("Black Player [KR]", "White Player [CN]"));
    assert_eq!(g.room_url.as_deref(), Some(ROOM));
    assert_eq!(g.status, "正在直播");
    assert_eq!(games[1].room_url, None, "a preview without moves cannot be followed");
    assert!(games[2].room_url.is_some(), "a live room is followed before its first move");
    assert_eq!(
        yike::parse_list(r#"{"Status":500,"Message":"busy"}"#),
        Err("busy".to_string())
    );
}

#[test]
fn detail_reads_sgf_then_clean_sgf_and_the_finished_state() {
    let live = yike::parse_detail(
        r#"{"status":0,"message":"","result":{"id":186031,"sgf":"(;GM[1]SZ[19];B[aa];W[bb])","status":2,"game_result":""}}"#,
    )
    .unwrap();
    assert_eq!((live.sgf.as_str(), live.finished), ("(;GM[1]SZ[19];B[aa];W[bb])", false));
    let done = yike::parse_detail(
        r#"{"status":0,"message":"","result":{"clean_sgf":"(;GM[1]SZ[19];B[cc])","status":3,"game_result":"B+R"}}"#,
    )
    .unwrap();
    assert_eq!((done.sgf.as_str(), done.finished, done.result.as_str()), ("(;GM[1]SZ[19];B[cc])", true, "B+R"));
}

#[test]
fn only_the_first_variation_of_a_live_sgf_is_the_main_line() {
    let plain = "(;GM[1]SZ[19];B[aa];W[bb])";
    assert_eq!(yike::mainline_only(plain), plain);
    assert_eq!(
        yike::mainline_only("(;GM[1]SZ[19];B[aa](;W[bb];B[cc])(;W[dd];B[ee]);W[ff])"),
        "(;GM[1]SZ[19];B[aa];W[bb];B[cc];W[ff])"
    );
    assert_eq!(
        yike::mainline_only("(;GM[1]C[text (not a tree) \\] ok];B[aa](;W[bb])(;W[cc]))"),
        "(;GM[1]C[text (not a tree) \\] ok];B[aa];W[bb])"
    );
}

#[test]
fn only_a_manual_proxy_without_host_or_port_is_unusable() {
    let manual = |host: &str, port| NetworkSettings { proxy_mode: ProxyMode::Manual, proxy_host: host.into(), proxy_port: port };
    assert_eq!(invalid_proxy_message(&NetworkSettings::default()), None);
    assert_eq!(invalid_proxy_message(&manual("127.0.0.1", 7897)), None);
    assert_eq!(
        invalid_proxy_message(&manual(" ", 7897)).as_deref(),
        Some("Network proxy settings are invalid. Open Settings > Advanced > Network Proxy and fix Invalid network-proxy-host: ")
    );
    assert!(invalid_proxy_message(&manual("h", 0)).unwrap().ends_with("Invalid network-proxy-port: 0"));
}

// ------------------------------------------------------------------ the follow flow

fn workbench(dir: &std::path::Path) -> Workbench {
    let storage = Storage::in_dirs(&dir.join("config"), &dir.join("data"));
    Workbench::new(storage, None, Box::new(NoEngines), Vec::new())
}

fn detail(moves: &str) -> String {
    detail_with("KM[7.5]", moves)
}

/// A detail reply with root properties `props` (komi and the like) and the moves.
fn detail_with(props: &str, moves: &str) -> String {
    let sgf = format!("(;GM[1]SZ[19]{props}PB[Shin]PW[Ke]{moves})");
    serde_json::json!({ "status": 0, "message": "", "result": { "sgf": sgf, "status": 2, "game_result": "" } })
        .to_string()
}

/// Runs the workbench timers like the platform's refresher for up to `span` and returns the reads
/// the flow asked for.
fn reads_within(wb: &mut Workbench, span: Duration) -> Vec<u64> {
    let until = Instant::now() + span;
    let mut reads = Vec::new();
    for _ in 0..50 {
        let Some(due) = wb.engine_timer().filter(|&d| d <= until) else { break };
        wb.engine_tick(due.max(Instant::now()));
        reads.extend(wb.take_online_jobs().into_iter().filter_map(|job| match job {
            OnlineJob::Get { request, url, .. } => {
                assert_eq!(url, "https://api-new.yikeweiqi.com/v1/golives/186031");
                Some(request)
            }
            _ => None,
        }));
        if !reads.is_empty() {
            break;
        }
    }
    reads
}

/// The room's next detail read (polled every second).
fn next_read(wb: &mut Workbench) -> u64 {
    let reads = reads_within(wb, Duration::from_secs(2));
    assert_eq!(reads.len(), 1, "one read at a time");
    reads[0]
}

fn reply(wb: &mut Workbench, request: u64, moves: &str) -> bool {
    wb.online_http(request, HttpReply::Ok { status: 200, body: detail(moves) })
}

fn reply_with(wb: &mut Workbench, request: u64, props: &str, moves: &str) -> bool {
    wb.online_http(request, HttpReply::Ok { status: 200, body: detail_with(props, moves) })
}

fn follow(wb: &mut Workbench, moves: &str) {
    wb.online(OnlineCommand::FollowLive { url: ROOM.into() }).unwrap();
    let r = next_read(wb);
    assert!(reply(wb, r, moves));
}

fn rec(wb: &Workbench) -> &GameRecord {
    &wb.document().record
}

fn main_line(wb: &Workbench) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut n = ROOT;
    while let Some(c) = rec(wb).main_child(n) {
        out.push(c);
        n = c;
    }
    out
}

fn p(s: &str) -> Point {
    let b = s.as_bytes();
    Point { x: u16::from(b[0] - b'a'), y: u16::from(b[1] - b'a') }
}

fn point(wb: &Workbench, n: NodeId) -> Point {
    match rec(wb).node(n).kind {
        NodeKind::Move { point, .. } => point,
        other => panic!("not a move: {other:?}"),
    }
}

#[test]
fn a_live_room_extends_the_main_line_and_keeps_what_the_user_added() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    follow(&mut wb, ";B[pd];W[dp]");
    assert_eq!(main_line(&wb).len(), 2);
    assert_eq!(wb.cursor(), main_line(&wb)[1], "the first sync shows the live end");
    assert_eq!((rec(&wb).info.black.as_str(), rec(&wb).info.white.as_str()), ("Shin", "Ke"));

    // The user studies a variation from the first move and comments on it.
    wb.navigate(Navigation::Back(1));
    let first = wb.cursor();
    assert_eq!(wb.edit(EditCommand::Play { point: p("qq"), color: None }).unwrap(), CommandResult::Done);
    let study = wb.cursor();
    assert_ne!(rec(&wb).main_child(first), Some(study));

    let r = next_read(&mut wb);
    assert!(reply(&mut wb, r, ";B[pd];W[dp];B[pp];W[dd]"));
    let line = main_line(&wb);
    assert_eq!(line.iter().map(|&n| point(&wb, n)).collect::<Vec<_>>(), [p("pd"), p("dp"), p("pp"), p("dd")]);
    assert!(rec(&wb).node(first).children.contains(&study), "the user's variation stays");
    assert_eq!(wb.cursor(), study, "browsing away from the live end is left alone");
}

#[test]
fn browsing_follows_new_moves_only_from_the_live_end() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    follow(&mut wb, ";B[pd]");
    let r = next_read(&mut wb);
    reply(&mut wb, r, ";B[pd];W[dp]");
    assert_eq!(wb.cursor(), main_line(&wb)[1], "at the live end, the cursor moves on");

    wb.navigate(Navigation::First);
    let r = next_read(&mut wb);
    reply(&mut wb, r, ";B[pd];W[dp];B[pp]");
    assert_eq!(wb.cursor(), ROOT, "elsewhere it stays");

    wb.online(OnlineCommand::SetAlwaysGotoLast { enabled: true }).unwrap();
    let r = next_read(&mut wb);
    reply(&mut wb, r, ";B[pd];W[dp];B[pp];W[dd]");
    assert_eq!(wb.cursor(), main_line(&wb)[3], "总是跳转到最新一手 always follows");
}

#[test]
fn a_retracted_live_move_goes_but_a_move_the_user_annotated_stays() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    follow(&mut wb, ";B[pd];W[dp];B[pp]");
    let line = main_line(&wb);
    wb.navigate(Navigation::Back(1));
    assert_eq!(wb.cursor(), line[1]);
    wb.edit(EditCommand::SetComment { node: line[1], comment: "mine".into() }).unwrap();

    let r = next_read(&mut wb);
    reply(&mut wb, r, ";B[pd];W[dq];B[qq]");
    let now = main_line(&wb);
    assert_eq!(now.iter().map(|&n| point(&wb, n)).collect::<Vec<_>>(), [p("pd"), p("dq"), p("qq")]);
    assert!(rec(&wb).contains(line[1]), "the annotated move is the user's now and stays");
    assert_eq!(rec(&wb).node(line[1]).comment, "mine");
}

#[test]
fn replies_of_an_ended_follow_change_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    follow(&mut wb, ";B[pd]");
    let stale = next_read(&mut wb);
    wb.online(OnlineCommand::Stop).unwrap();
    let revision = wb.document().revision;
    assert!(!reply(&mut wb, stale, ";B[pd];W[dp]"));
    assert_eq!(wb.document().revision, revision);
    assert!(reads_within(&mut wb, Duration::from_secs(3)).is_empty(), "nothing is polled any more");
    assert!(wb.view(true).online.following.is_none());

    // A new follow does not take the old follow's reply either.
    follow(&mut wb, ";B[pd]");
    let other = wb.document().revision;
    assert!(!reply(&mut wb, stale, ";B[pd];W[dp];B[pp]"));
    assert_eq!(wb.document().revision, other);
}

#[test]
fn opening_another_record_ends_the_follow() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    follow(&mut wb, ";B[pd]");
    let pending = next_read(&mut wb);
    let t = wb.begin_open(OpenOrigin::File { path: "/x.sgf".into(), source: lizzie_core::api::OpenSource::Drop });
    let read = formats::parse(RecordFormat::Sgf, "(;SZ[9];B[ee])", false);
    assert!(wb.complete_open(t, read.map(|r| (RecordFormat::Sgf, r))));
    reply(&mut wb, pending, ";B[pd];W[dp]");
    assert_eq!(rec(&wb).size.width, 9, "the opened record is untouched");
    assert!(wb.view(true).online.following.is_none());

    // The same before the room's first reply.
    wb.online(OnlineCommand::FollowLive { url: ROOM.into() }).unwrap();
    let first = next_read(&mut wb);
    let t = wb.begin_open(OpenOrigin::File { path: "/y.sgf".into(), source: lizzie_core::api::OpenSource::Drop });
    let read = formats::parse(RecordFormat::Sgf, "(;SZ[13];B[gg])", false);
    assert!(wb.complete_open(t, read.map(|r| (RecordFormat::Sgf, r))));
    let revision = wb.document().revision;
    reply(&mut wb, first, ";B[pd];W[dp]");
    assert_eq!((rec(&wb).size.width, wb.document().revision), (13, revision), "the first reply changes nothing");
    assert!(wb.view(true).online.following.is_none());
}

#[test]
fn clearing_the_board_ends_the_follow_even_before_its_first_game() {
    // Board.clearBoardState(false) defers urlSgfStopSyncAction (Board.java:5505-5525): a follow
    // ends with any board clear that is not an engine match's, as soon as the sync started.
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.online(OnlineCommand::FollowLive { url: ROOM.into() }).unwrap();
    let first = next_read(&mut wb);
    assert_eq!(wb.edit(EditCommand::ClearBoard).unwrap(), CommandResult::Done);
    assert!(wb.view(true).online.following.is_none(), "the clear stopped the follow");
    let (doc, revision) = (wb.document().id, wb.document().revision);
    reply(&mut wb, first, ";B[pd];W[dp]");
    assert_eq!((wb.document().id, wb.document().revision), (doc, revision), "the room's game does not replace the cleared board");
    assert!(main_line(&wb).is_empty());
    assert!(reads_within(&mut wb, Duration::from_secs(3)).is_empty(), "nothing is polled any more");
}

#[test]
fn a_later_source_komi_applies_unless_the_user_set_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    follow(&mut wb, ";B[pd]");
    assert_eq!(rec(&wb).info.komi, 7.5);
    let r = next_read(&mut wb);
    reply_with(&mut wb, r, "KM[6.5]", ";B[pd];W[dp]");
    assert_eq!((rec(&wb).info.komi, wb.view(true).komi), (6.5, 6.5), "the corrected komi reaches the record");

    wb.set_komi(0.5).unwrap();
    let r = next_read(&mut wb);
    reply_with(&mut wb, r, "KM[6.5]", ";B[pd];W[dp];B[pp]");
    assert_eq!(rec(&wb).info.komi, 0.5, "a komi set by hand stays");
    // A game the document cannot extend (another size) is adopted again; the hand-set komi stays
    // through that and the next update.
    let r = next_read(&mut wb);
    wb.online_http(r, HttpReply::Ok { status: 200, body: detail_with("KM[6.5]", ";B[pd]").replace("SZ[19]", "SZ[13]") });
    assert_eq!((rec(&wb).size.width, rec(&wb).info.komi), (13, 0.5));
    let r = next_read(&mut wb);
    wb.online_http(r, HttpReply::Ok { status: 200, body: detail_with("KM[6.5]", ";B[pd];W[dd]").replace("SZ[19]", "SZ[13]") });
    assert_eq!(rec(&wb).info.komi, 0.5);
}

#[test]
fn loading_an_address_that_is_no_room_does_not_arm_the_web_window() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    for lobby in ["https://home.yikeweiqi.com/#/live", "  "] {
        wb.online(OnlineCommand::BrowserLoad { url: lobby.into() }).unwrap();
        let o = wb.view(true).online;
        assert!(!o.browser_sync, "{lobby:?}");
        assert_eq!(o.browser_status.map(|s| s.args), Some(vec!["URL 无法识别".to_string()]), "{lobby:?}");
    }
    wb.online(OnlineCommand::BrowserLoad { url: ROOM.into() }).unwrap();
    let o = wb.view(true).online;
    assert!(o.browser_sync && o.following.is_some_and(|f| f.url == ROOM));
}

#[test]
fn a_failed_read_keeps_polling_but_an_unusable_proxy_stops_and_warns() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    follow(&mut wb, ";B[pd]");
    let r = next_read(&mut wb);
    assert!(wb.online_http(r, HttpReply::Failed { proxy: false, message: "timed out".into() }));
    assert_eq!(wb.view(true).online.proxy_warning, None);

    let r = next_read(&mut wb);
    let message = "Network proxy settings are invalid. Open Settings > Advanced > Network Proxy and fix Invalid network-proxy-port: 0";
    assert!(wb.online_http(r, HttpReply::Failed { proxy: true, message: message.into() }));
    let warning = wb.view(true).online.proxy_warning.expect("网络代理设置 is shown");
    assert_eq!(warning.message, message);
    assert!(reads_within(&mut wb, Duration::from_secs(3)).is_empty(), "the room is no longer polled");
}

#[test]
fn the_live_list_reports_failures_without_the_proxy_warning() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.online(OnlineCommand::LoadLiveList { category: LiveCategory::default(), more: false }).unwrap();
    let request = match wb.take_online_jobs().as_slice() {
        [OnlineJob::Get { request, url, .. }] => {
            assert!(url.starts_with("https://api.yikeweiqi.com/v2/golive/list?"), "{url}");
            *request
        }
        other => panic!("{other:?}"),
    };
    assert!(wb.view(true).online.live.loading);
    assert!(wb.online_http(request, HttpReply::Failed { proxy: true, message: "bad proxy".into() }));
    let online = wb.view(true).online;
    assert!(!online.live.loading);
    assert_eq!(online.proxy_warning, None);
    let status = online.live.status.expect("a failure status");
    assert!(status.args.iter().any(|a| a == "bad proxy"), "{status:?}");
}

#[test]
fn a_plain_link_is_read_once_as_a_record_of_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.online(OnlineCommand::Open { url: "https://example.com/a.sgf".into(), refresh_seconds: 0 }).unwrap();
    let request = match wb.take_online_jobs().as_slice() {
        [OnlineJob::Get { request, url, headers, .. }] => {
            assert_eq!(url, "https://example.com/a.sgf");
            assert!(headers.iter().any(|(k, _)| k == "User-Agent"));
            *request
        }
        other => panic!("{other:?}"),
    };
    assert!(wb.online_http(request, HttpReply::Ok { status: 200, body: "(;SZ[13];B[gg];W[cc])".into() }));
    assert_eq!(rec(&wb).size.width, 13);
    assert_eq!(main_line(&wb).len(), 2);
    assert!(wb.view(true).online.following.is_none());
    assert!(reads_within(&mut wb, Duration::from_secs(3)).is_empty(), "nothing is read again");
}

/// A 弈客 Socket.IO room (type 1) joined through `online_socket`, as the platform relays it.
fn joined_room(wb: &mut Workbench) -> u64 {
    joined_room_with(wb, "(;GM[1]SZ[19];B[pd];W[dp])")
}

fn joined_room_with(wb: &mut Workbench, sgf: &str) -> u64 {
    wb.online(OnlineCommand::FollowLive { url: "https://home.yikeweiqi.com/#/live/room/18328/1/123".into() }).unwrap();
    let mut conn = None;
    for _ in 0..5 {
        conn = wb.take_online_jobs().into_iter().find_map(|j| match j {
            OnlineJob::Connect { conn, .. } => Some(conn),
            _ => None,
        });
        if conn.is_some() {
            break;
        }
        let due = wb.engine_timer().expect("a connect is due");
        wb.engine_tick(due.max(Instant::now()));
    }
    let conn = conn.expect("the room connects");
    assert!(wb.online_socket(conn, SocketEvent::Opened));
    let mut text = |t: &str| assert!(wb.online_socket(conn, SocketEvent::Text(t.into())), "{t}");
    text(r#"0{"sid":"eio","upgrades":[],"pingInterval":25000,"pingTimeout":20000}"#);
    text(r#"40{"sid":"sio"}"#);
    text(r#"42["userinfo",{"user_id":77}]"#);
    text(r#"431[{"code":0}]"#);
    text(r#"432[{"code":0}]"#);
    text(r#"433[{"code":0,"data":[]}]"#);
    let init = serde_json::json!(["init", { "game_info": {
        "sgf": sgf, "blackName": "甲", "whiteName": "乙", "boardSize": 19, "status": 1, "resultDesc": "" } }]);
    text(&format!("42{init}"));
    conn
}

#[test]
fn an_updated_commentator_line_replaces_only_itself() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let conn = joined_room(&mut wb);
    let first = main_line(&wb)[0];
    let comment = |content: &str, id: u32, kind: &str| {
        format!(r#"42["comments",{{"type":"{kind}","content":{{"type":2,"handsCount":1,"id":{id},"content":"{content}","extend":"{{\"LiveMember\":\"解说\"}}"}}}}]"#)
    };
    wb.online_socket(conn, SocketEvent::Text(comment("好棋", 5, "add")));
    wb.online_socket(conn, SocketEvent::Text(comment("开局", 6, "add")));
    let mine = format!("{}\n我的笔记", rec(&wb).node(first).comment);
    wb.edit(EditCommand::SetComment { node: first, comment: mine }).unwrap();

    wb.online_socket(conn, SocketEvent::Text(comment("妙手", 5, "update")));
    assert_eq!(rec(&wb).node(first).comment, "解说：妙手\n解说：开局\n我的笔记");
}

#[test]
fn a_node_the_user_annotated_survives_a_later_source_comment_and_takeback() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let conn = joined_room(&mut wb);
    let second = main_line(&wb)[1];
    wb.edit(EditCommand::SetComment { node: second, comment: "我的笔记".into() }).unwrap();
    wb.online_socket(
        conn,
        SocketEvent::Text(r#"42["comments",{"type":"add","content":{"type":2,"handsCount":2,"id":7,"content":"好","extend":"{\"LiveMember\":\"解说\"}"}}]"#.into()),
    );
    assert_eq!(rec(&wb).node(second).comment, "我的笔记\n解说：好");
    // The room takes its second move back.
    wb.online_socket(
        conn,
        SocketEvent::Text(r#"42["init",{"game_info":{"sgf":"(;GM[1]SZ[19];B[pd])","boardSize":19,"status":1,"resultDesc":""}}]"#.into()),
    );
    assert!(rec(&wb).contains(second), "the annotated move stays");
    assert!(rec(&wb).node(second).comment.contains("我的笔记"));
}

#[test]
fn a_commentator_variation_attaches_relative_to_its_move_with_colours_passes_and_comments() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let conn = joined_room(&mut wb);
    let first = main_line(&wb)[0];
    let main = main_line(&wb);
    // listenBack / comments type 1: Java SGFParser.parseBranch reads the content from the node
    // at handsCount.
    wb.online_socket(
        conn,
        SocketEvent::Text(r#"42["comments",{"type":"add","content":{"type":1,"handsCount":1,"id":9,"content":";W[qq]C[看这里];B[]"}}]"#.into()),
    );
    assert_eq!(main_line(&wb), main, "the main line stays");
    let branch = *rec(&wb).node(first).children.last().unwrap();
    assert_eq!(rec(&wb).node(branch).kind, NodeKind::Move { color: lizzie_core::record::Stone::White, point: p("qq") });
    assert_eq!(rec(&wb).node(branch).comment, "看这里");
    let next = rec(&wb).node(branch).children[0];
    assert_eq!(rec(&wb).node(next).kind, NodeKind::Pass { color: lizzie_core::record::Stone::Black });
}

#[test]
fn a_commentator_variation_is_read_on_the_position_of_its_move() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let conn = joined_room_with(&mut wb, "(;GM[1]SZ[19]AB[aa][ac][ca][cc][bd][db];B[ss])");
    let first = main_line(&wb)[0];
    assert_eq!(main_line(&wb).len(), 1);
    // W[ba] takes the corner stone A19; the last black move then takes C18 and B17. On an empty
    // board it would be suicide: it is legal only on the room's position.
    wb.online_socket(
        conn,
        SocketEvent::Text(r#"42["comments",{"type":"add","content":{"type":1,"handsCount":1,"id":9,"content":";W[ab];B[];W[ba];B[];W[cb];B[];W[bc];B[bb]C[capture]"}}]"#.into()),
    );
    // The last move had no continuation: the variation is a side branch after an end placeholder.
    let after = rec(&wb).main_child(first).expect("placeholder");
    assert_eq!(rec(&wb).node(after).kind, NodeKind::DummyPass);
    let r = rec(&wb);
    let mut at = *r.node(first).children.last().unwrap();
    let mut actions = vec![r.node(at).kind];
    while let Some(&next) = r.node(at).children.first() {
        at = next;
        actions.push(r.node(at).kind);
    }
    assert_eq!(actions.len(), 8, "{actions:?}");
    assert_eq!(r.node(at).kind, NodeKind::Move { color: lizzie_core::record::Stone::Black, point: p("bb") });
    assert_eq!(r.node(at).comment, "capture");
    let pos = r.position(at);
    assert_eq!((pos.white_captures, pos.black_captures), (1, 2));
}

#[test]
fn an_unusable_proxy_on_the_broadcast_socket_warns_and_stops_reconnecting() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.online(OnlineCommand::FollowLive { url: "https://h5.foxwq.com/share?gameid=12&createtime=34".into() }).unwrap();
    let connects = |wb: &mut Workbench| -> Vec<u64> {
        let mut found = Vec::new();
        let until = Instant::now() + Duration::from_secs(3);
        for _ in 0..50 {
            found.extend(wb.take_online_jobs().into_iter().filter_map(|j| match j {
                OnlineJob::Connect { conn, .. } => Some(conn),
                _ => None,
            }));
            let Some(due) = wb.engine_timer().filter(|&d| d <= until) else { break };
            wb.engine_tick(due.max(Instant::now()));
        }
        found
    };
    let conn = connects(&mut wb)[0];
    let message = "Network proxy settings are invalid. Open Settings > Advanced > Network Proxy and fix Invalid network-proxy-port: 0".to_string();
    wb.online_socket(conn, SocketEvent::Refused { message: message.clone() });
    let warning = wb.view(true).online.proxy_warning.expect("网络代理设置 is shown");
    assert_eq!(warning.message, message);
    assert!(connects(&mut wb).is_empty(), "no reconnect with the unusable settings");
}

#[test]
fn a_room_the_server_disconnects_or_refuses_stops_following_and_keeps_the_record() {
    // Socket.IO `41` (the server disconnects the namespace) and `44` (it refuses the connect)
    // end the room for good; the follow ends with them while a dropped transport reconnects.
    for packet in ["41", r#"44{"message":"Invalid namespace"}"#] {
        let dir = tempfile::tempdir().unwrap();
        let mut wb = workbench(dir.path());
        let conn = joined_room(&mut wb);
        let kept = main_line(&wb);
        wb.take_online_jobs();
        assert!(wb.online_socket(conn, SocketEvent::Text(packet.into())), "{packet}");
        let o = wb.view(true).online;
        assert!(o.following.is_none(), "{packet}: the follow ended");
        let stopped = OnlineText::with("Online.text", ["已停止同步".to_string()]);
        assert_eq!(o.browser_status, Some(stopped), "{packet}: Java's stopSync status");
        assert_eq!(main_line(&wb), kept, "{packet}: the record stays");
        let jobs = wb.take_online_jobs();
        assert!(!jobs.iter().any(|j| matches!(j, OnlineJob::Connect { .. })), "{packet}: {jobs:?}");
        if let Some(due) = wb.engine_timer() {
            wb.engine_tick(due.max(Instant::now()));
            assert!(!wb.take_online_jobs().iter().any(|j| matches!(j, OnlineJob::Connect { .. })), "{packet}");
        }
    }
}

#[test]
fn a_dropped_room_connection_still_reconnects() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let conn = joined_room(&mut wb);
    wb.take_online_jobs();
    assert!(wb.online_socket(conn, SocketEvent::Closed { error: None }));
    assert!(wb.view(true).online.following.is_some());
    let until = Instant::now() + Duration::from_secs(10);
    let mut reconnected = false;
    for _ in 0..50 {
        let Some(due) = wb.engine_timer().filter(|&d| d <= until) else { break };
        wb.engine_tick(due.max(Instant::now()));
        if wb.take_online_jobs().iter().any(|j| matches!(j, OnlineJob::Connect { .. })) {
            reconnected = true;
            break;
        }
    }
    assert!(reconnected, "a dropped transport reconnects after its backoff");
}

#[test]
fn a_unite_reply_is_taken_only_by_the_room_it_was_read_for() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let unite = |room: u64| format!("https://home.yikeweiqi.com/#/unite/{room}");
    let reply = |room: u64, sgf: &str| PageReport::Unite {
        room,
        tag: "resp".into(),
        status: 200,
        body: serde_json::json!({ "code": 0, "data": { "sgf": sgf, "hands_count": 1 } }).to_string(),
        message: String::new(),
    };
    wb.online(OnlineCommand::Open { url: unite(111), refresh_seconds: 5 }).unwrap();
    wb.online(OnlineCommand::Open { url: unite(222), refresh_seconds: 5 }).unwrap();
    let before = (wb.document().id, wb.document().revision);
    // Room 111's reply, read before the switch, arrives late.
    assert!(!wb.online_page(reply(111, "(;GM[1]SZ[19];B[aa])")));
    assert_eq!((wb.document().id, wb.document().revision), before, "room 222's follow ignores it");
    assert!(wb.online_page(reply(222, "(;GM[1]SZ[19];B[bb])")));
    assert_eq!(main_line(&wb).len(), 1, "room 222's own reply is its game");
}

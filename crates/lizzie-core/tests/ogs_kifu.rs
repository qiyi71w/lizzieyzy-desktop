//! OGS 棋谱 through the public workbench interface with a scripted transport: Java
//! `OgsGameClient` / `OgsSgfClient` requests and reply reading, the window's search, paging and
//! record download through the ordinary open path, cancelling on close, the error classes, and
//! 打开在线链接 with OGS addresses.
//!
//! Fixtures under `fixtures/ogs_kifu` are real OGS public API replies, recorded once as
//! `manifest.json` lists them (each request's address, status and body hash). The other replies
//! here are the protocol inputs of Java `OgsGameClientTest` / `OgsSgfClientTest` and status-only
//! replies for the error classes the recording has no sample of (429, no reply, no games).

use std::collections::VecDeque;

use lizzie_core::api::{
    CommandResult, KifuPage, OgsKifuCommand, OgsKifuLabel, OgsKifuStatus, OgsKifuView, OnlineCommand, OnlineJob,
    OpenOutcome, OpenSource,
};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::formats::sgf;
use lizzie_core::ogs_kifu::{fetch, link_game, result, Http, OgsFailure, Player, Reply, Request, Response, API};
use lizzie_core::online::HttpReply;
use lizzie_core::record::{NodeKind, Point, RecordFormat, Stone, ROOT};
use lizzie_core::settings::{KifuAfterGet, Storage};
use lizzie_core::workbench::{OgsKifuJob, OgsKifuTask, OpenOrigin, Workbench, OGS_LINK_FAILED, OGS_LINK_RATE_LIMITED};
use serde_json::json;

struct NoEngines;
impl Launcher for NoEngines {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        Err(std::io::Error::other("no engines"))
    }
}

fn workbench(dir: &std::path::Path) -> Workbench {
    let storage = Storage::in_dirs(&dir.join("config"), &dir.join("data"));
    Workbench::new(storage, None, Box::new(NoEngines), Vec::new())
}

/// Answers requests in order: each expected address with its reply (`Err`: no reply at all).
#[derive(Default)]
struct Script {
    replies: VecDeque<(String, Result<Response, String>)>,
    asked: Vec<String>,
}
impl Script {
    fn reply(mut self, url: impl Into<String>, status: u16, body: impl Into<Vec<u8>>) -> Script {
        self.replies.push_back((
            url.into(),
            Ok(Response {
                status,
                body: body.into(),
            }),
        ));
        self
    }
    fn fail(mut self, url: impl Into<String>) -> Script {
        self.replies.push_back((url.into(), Err("connection refused".into())));
        self
    }
}
impl Http for Script {
    fn get(&mut self, url: &str) -> Result<Response, String> {
        self.asked.push(url.to_string());
        let (expected, reply) = self
            .replies
            .pop_front()
            .unwrap_or_else(|| panic!("unexpected request {url}"));
        assert_eq!(url, expected);
        reply
    }
}

const BASE: &str = "http://127.0.0.1:9/api/v1/";

fn games_url(player: u64, page: u32) -> String {
    format!("{BASE}players/{player}/games/?ended__isnull=false&ordering=-ended&page_size=20&page={page}")
}

/// One listed game as `OgsGameClientTest` writes it.
fn game(id: u64, white: &str) -> serde_json::Value {
    json!({"id": id, "ended": "2026-01-01T12:00:00Z", "players": {"black": {"username": "黒"}, "white": {"username": white}},
           "black_lost": false, "white_lost": true, "outcome": "Resignation", "width": 19, "height": 19})
}

fn page(count: u64, games: Vec<serde_json::Value>) -> String {
    json!({"count": count, "next": "https://untrusted.test/page", "results": games}).to_string()
}

/// The record of `OgsSgfClientTest.downloadsUtf8AndClosesResources`, with its byte-order mark.
const SGF: &str = "\u{feff}(;FF[4]GM[1]SZ[19]KM[6.5]PB[黒]PW[白];B[pd])\n";

fn serve(wb: &mut Workbench, http: &mut Script) -> Vec<OgsKifuJob> {
    serve_at(wb, http, BASE)
}

fn serve_at(wb: &mut Workbench, http: &mut Script, base: &str) -> Vec<OgsKifuJob> {
    let mut served = Vec::new();
    for task in wb.take_ogs_kifu_tasks() {
        if let OgsKifuTask::Fetch(job) = task {
            let reply = fetch(&job.request, base, http, job.no_capture);
            wb.ogs_kifu_reply(&job, reply);
            served.push(job);
        }
    }
    assert!(http.replies.is_empty(), "unused replies: {:?}", http.replies);
    served
}

fn op(wb: &mut Workbench, op: OgsKifuCommand) -> CommandResult {
    wb.ogs_kifu(op).unwrap()
}

fn view(wb: &mut Workbench) -> OgsKifuView {
    wb.view(false).ogs_kifu
}

fn open_file(wb: &mut Workbench, text: &str) -> bool {
    let ticket = wb.begin_open(OpenOrigin::File {
        path: "/x.sgf".into(),
        source: OpenSource::Drop,
    });
    wb.complete_open(ticket, Ok((RecordFormat::Sgf, sgf::read(text).unwrap())))
}

/// Searches `alice` (player 12) and lists page 1 of 3.
fn listed(wb: &mut Workbench) {
    op(wb, OgsKifuCommand::Search { text: "alice".into() });
    let mut http = Script::default()
        .reply(
            format!("{BASE}players/?username=alice"),
            200,
            json!({"results": [{"id": 12, "username": "Alice"}]}).to_string(),
        )
        .reply(
            games_url(12, 1),
            200,
            page(41, vec![game(101, "White"), game(102, "W2")]),
        );
    serve(wb, &mut http);
}

/// Each address as Java `OgsSgfClient.sgfUri` reads it: `URI.create` on the whole trimmed text
/// (an invalid escape or character anywhere makes it no OGS link), then the decoded path; the ID
/// is the text of its digits, whatever its length.
#[test]
fn links_to_ogs_games_and_sgf_downloads_name_their_game() {
    for (input, game) in [
        (" https://online-go.com/game/123 ", "123"),
        ("\u{1}https://online-go.com/game/123", "123"),
        ("http://www.online-go.com/game/123/?move=5#20", "123"),
        ("https://online-go.com/api/v1/games/123/sgf", "123"),
        ("https://online-go.com/api/v1/games/123/sgf/123.sgf", "123"),
        ("https://www.online-go.com/api/v1/games/5/sgf/5.sgf/", "5"),
        ("HTTPS://Online-Go.com/game/123", "123"),
        ("https://online-go.com/game/68984609", "68984609"),
        (
            "https://online-go.com/game/18446744073709551616",
            "18446744073709551616",
        ),
        (
            "https://online-go.com/api/v1/games/99999999999999999999999/sgf/1.sgf",
            "99999999999999999999999",
        ),
        // An empty port is no port.
        ("https://online-go.com:/game/123", "123"),
        // The path is matched decoded.
        ("https://online-go.com/game/%31%32%33", "123"),
        ("https://online-go.com/game%2F123", "123"),
        // Any valid query or fragment.
        ("https://online-go.com/game/123?q=[x]", "123"),
        ("https://online-go.com/game/123?x=1&y=$,;:@+=!~*'()", "123"),
        ("https://online-go.com/game/123?a#b?c", "123"),
        ("https://online-go.com/game/123?%aF", "123"),
        ("https://online-go.com/game/123?名前=碁#名前", "123"),
        ("https://online-go.com/game/123?\u{1F600}", "123"),
        ("https://online-go.com/game/123?#", "123"),
    ] {
        assert_eq!(link_game(input).as_deref(), Some(game), "{input}");
    }
    for input in [
        "https://online-go.com.evil.test/game/123",
        "https://evil.test/online-go.com/game/123",
        "https://user@online-go.com/game/123",
        "https://@online-go.com/game/123",
        "https://online-go.com:8443/game/123",
        "https://online-go.com:443/game/123",
        "https://online-go.com::/game/123",
        "https://online-go.com./game/123",
        "https://online%2Dgo.com/game/123",
        "ftp://online-go.com/game/123",
        "https:online-go.com/game/123",
        "https:/online-go.com/game/123",
        "https:///game/123",
        "https://online-go.com/",
        "https://online-go.com/game/0",
        "https://online-go.com/game/-1",
        "https://online-go.com/game/abc",
        "https://online-go.com/game/012",
        "https://online-go.com/GAME/123",
        "https://online-go.com/player/123",
        "https://online-go.com/review/123",
        "https://online-go.com/api/v1/games/123",
        "https://online-go.com/api/v1/games/123/sgf/x.sgf",
        "https://online-go.com/api/v1/games/5/sgf/05.sgf",
        "https://online-go.com/game/123//",
        "https://online-go.com/game/123;x",
        "https://online-go.com/game/123%3F",
        "https://online-go.com/game/%FF",
        // Invalid escapes anywhere.
        "https://online-go.com/game/123?x=%ZZ",
        "https://online-go.com/game/123?x=%2",
        "https://online-go.com/game/123#%ZZ",
        "https://online-go.com/game/123%",
        // Characters a URI cannot hold, in any part.
        "https://online-go.com/game/[123]",
        "https://online-go.com/game/1{2}",
        "https://online-go.com/game/123#a#b",
        "https://online-go.com/game/123?a b",
        "https://online-go.com/game/123?a|b",
        "https://online-go.com/game/123?a\"b",
        "https://online-go.com/game/123?a^b",
        "https://online-go.com/game/123?a\\b",
        "https://online-go.com/game/123?`",
        "https://online-go.com/game/123?<x>",
        "https://online-go.com/game/123?\u{a0}",
        "https://online-go.com/game/123?\u{80}",
        "https://online-go.com/game/123?\u{2028}",
        "https://online-go.com/game/123\u{3000}",
        "\u{3000}https://online-go.com/game/123",
        "h ttp://online-go.com/game/7",
        "not a URL",
        "",
    ] {
        assert_eq!(link_game(input), None, "{input}");
    }
}

/// 打开在线链接 with an address Java takes for an OGS game imports it however long its ID; one Java
/// does not take (an invalid escape) follows the ordinary path.
#[test]
fn open_online_link_reads_the_whole_address_as_java_does() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let before = wb.document().id;

    let huge = "https://online-go.com/game/18446744073709551616";
    wb.online(OnlineCommand::Open {
        url: huge.into(),
        refresh_seconds: 1,
    })
    .unwrap();
    assert!(wb.take_online_jobs().is_empty(), "no ordinary request");
    assert!(wb.view(false).online.following.is_none());
    let mut http = Script::default().reply(format!("{BASE}games/18446744073709551616/sgf"), 404, "");
    serve(&mut wb, &mut http);
    assert_eq!(http.asked.len(), 1, "the game's SGF is asked for once");
    let Some(OpenOutcome::Failed { message_key, .. }) = wb.view(false).open_outcome else {
        panic!("the OGS message")
    };
    assert_eq!(message_key, OGS_LINK_FAILED);
    assert_eq!(wb.document().id, before, "the current game stays");

    let invalid = "https://online-go.com/game/123?x=%ZZ";
    wb.online(OnlineCommand::Open {
        url: invalid.into(),
        refresh_seconds: 1,
    })
    .unwrap();
    assert!(wb.take_ogs_kifu_tasks().is_empty(), "no OGS import");
    assert_eq!(wb.view(false).online.following.map(|f| f.url).as_deref(), Some(invalid));
}

#[test]
fn a_username_is_matched_exactly_and_an_id_is_looked_up_directly() {
    // The username goes as a form-encoded query; only an exact match (letter case aside) counts.
    let mut http = Script::default()
        .reply(
            format!("{BASE}players/?username=Alice+%26+Bob"),
            200,
            json!({"results": [{"id": 3, "username": "Alice & Bobby"}, {"id": 12, "username": "alice & bob"}]})
                .to_string(),
        )
        .reply(games_url(12, 1), 200, page(0, vec![]));
    let Reply::Found { player, page: first } = fetch(
        &Request::Search {
            query: " Alice & Bob ".into(),
        },
        BASE,
        &mut http,
        false,
    ) else {
        panic!("found")
    };
    assert_eq!(
        player,
        Player {
            id: 12,
            name: "alice & bob".into()
        }
    );
    assert!(first.games.is_empty());
    assert_eq!((first.page, first.pages), (1, 1));

    let mut http = Script::default()
        .reply(
            format!("{BASE}players/12/"),
            200,
            json!({"id": 12, "username": "Alice"}).to_string(),
        )
        .reply(games_url(12, 1), 200, page(41, vec![game(99, "白")]));
    let Reply::Found { player, page: first } = fetch(&Request::Search { query: "12".into() }, BASE, &mut http, false)
    else {
        panic!("found")
    };
    assert_eq!(player.name, "Alice");
    assert_eq!((first.page, first.pages), (1, 3), "41 games are 3 pages of 20");
    let g = &first.games[0];
    assert_eq!(
        (
            g.id,
            g.black.as_str(),
            g.white.as_str(),
            g.result.as_str(),
            g.size.as_str()
        ),
        (99, "黒", "白", "B+R", "19×19")
    );
    assert_eq!(g.ended, "2026-01-01T12:00:00Z");

    // A leading zero is a username, as in Java (`[1-9][0-9]*`).
    let mut http = Script::default().reply(
        format!("{BASE}players/?username=012"),
        200,
        json!({"results": []}).to_string(),
    );
    assert!(matches!(
        fetch(&Request::Search { query: "012".into() }, BASE, &mut http, false),
        Reply::NoPlayer
    ));
}

#[test]
fn each_failure_has_its_own_class() {
    let search = |http: &mut Script| fetch(&Request::Search { query: "alice".into() }, BASE, http, false);
    let by_name = format!("{BASE}players/?username=alice");
    // No reply at all: a network error.
    assert!(matches!(
        search(&mut Script::default().fail(&by_name)),
        Reply::Failed(OgsFailure::Network)
    ));
    // HTTP 429: OGS limits requests.
    assert!(matches!(
        search(&mut Script::default().reply(&by_name, 429, "")),
        Reply::Failed(OgsFailure::RateLimited)
    ));
    // Another status, a redirect (not followed), or a reply that is not the JSON asked for.
    for (status, body) in [
        (500, "{}"),
        (301, ""),
        (403, "{}"),
        (200, "<html>error</html>"),
        (200, "{\"results\":[{\"id\":1}]}"),
    ] {
        let reply = search(&mut Script::default().reply(&by_name, status, body));
        assert!(
            matches!(reply, Reply::Failed(OgsFailure::Unavailable)),
            "{status} {body}"
        );
    }
    // A username nobody has, and an ID OGS answers with 404: no such player.
    assert!(matches!(
        search(&mut Script::default().reply(
            &by_name,
            200,
            json!({"results": [{"id": 1, "username": "other"}]}).to_string()
        )),
        Reply::NoPlayer
    ));
    let mut http = Script::default().reply(format!("{BASE}players/999/"), 404, "{\"detail\":\"Not found.\"}");
    assert!(matches!(
        fetch(&Request::Search { query: "999".into() }, BASE, &mut http, false),
        Reply::NoPlayer
    ));
    // An ID reply without a positive id is unusable, never another player.
    let mut http = Script::default().reply(
        format!("{BASE}players/12/"),
        200,
        json!({"id": 0, "username": "bad"}).to_string(),
    );
    assert!(matches!(
        fetch(&Request::Search { query: "12".into() }, BASE, &mut http, false),
        Reply::Failed(OgsFailure::Unavailable)
    ));
    // A game list without its count, or a game without an id, is unusable; a 429 on a page too.
    let alice = Player {
        id: 12,
        name: "alice".into(),
    };
    let page_of = |http: &mut Script| {
        fetch(
            &Request::Page {
                player: alice.clone(),
                page: 2,
            },
            BASE,
            http,
            false,
        )
    };
    assert!(matches!(
        page_of(&mut Script::default().reply(games_url(12, 2), 200, "{\"count\":2}")),
        Reply::Failed(OgsFailure::Unavailable)
    ));
    let no_id = json!({"count": 1, "results": [{"ended": "x"}]}).to_string();
    assert!(matches!(
        page_of(&mut Script::default().reply(games_url(12, 2), 200, no_id)),
        Reply::Failed(OgsFailure::Unavailable)
    ));
    assert!(matches!(
        page_of(&mut Script::default().reply(games_url(12, 2), 429, "")),
        Reply::Failed(OgsFailure::RateLimited)
    ));
    assert!(matches!(
        page_of(&mut Script::default().fail(games_url(12, 2))),
        Reply::Failed(OgsFailure::Network)
    ));
}

#[test]
fn a_record_reply_must_be_sgf_text_within_the_size_limit() {
    let record = |status: u16, body: Vec<u8>| {
        let url = format!("{BASE}games/7/sgf");
        fetch(
            &Request::Record { game: "7".into() },
            BASE,
            &mut Script::default().reply(url, status, body),
            false,
        )
    };
    let Reply::Record(Ok(rec)) = record(200, SGF.into()) else {
        panic!("a record")
    };
    assert_eq!(
        (rec.info.black.as_str(), rec.info.white.as_str(), rec.info.komi),
        ("黒", "白", 6.5)
    );
    for body in ["", "<html>Sign in</html>", "{\"error\":true}", "(;SZ[19]"] {
        assert!(
            matches!(record(200, body.into()), Reply::Failed(OgsFailure::Unavailable)),
            "{body}"
        );
    }
    for status in [301, 403, 404, 500] {
        assert!(
            matches!(record(status, SGF.into()), Reply::Failed(OgsFailure::Unavailable)),
            "{status}"
        );
    }
    assert!(matches!(
        record(429, Vec::new()),
        Reply::Failed(OgsFailure::RateLimited)
    ));
    let mut huge = b"(;C[".to_vec();
    huge.resize(8 * 1024 * 1024 + 1, b'x');
    huge.extend_from_slice(b"])");
    assert!(matches!(record(200, huge), Reply::Failed(OgsFailure::Unavailable)));
}

#[test]
fn results_name_the_winner_and_how_the_game_ended() {
    let r = |v: serde_json::Value| result(v.as_object().unwrap());
    assert_eq!(
        r(json!({"black_lost": true, "white_lost": false, "outcome": "Timeout"})),
        "W+T"
    );
    assert_eq!(
        r(json!({"black_lost": true, "white_lost": false, "outcome": "6.5"})),
        "W+6.5"
    );
    assert_eq!(
        r(json!({"black_lost": false, "white_lost": true, "outcome": "resignation"})),
        "B+R"
    );
    assert_eq!(
        r(json!({"black_lost": true, "white_lost": false, "outcome": "6.5", "annulled": true})),
        "Void"
    );
    assert_eq!(
        r(json!({"black_lost": false, "white_lost": false, "outcome": "Draw"})),
        "Draw"
    );
}

#[test]
fn a_search_lists_pages_and_remembers_the_text() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    op(&mut wb, OgsKifuCommand::Open);
    let v = view(&mut wb);
    assert_eq!(
        (v.label.clone(), v.rows.len(), v.listed, v.busy),
        (OgsKifuLabel::Hint, 0, false, false)
    );
    // A blank field asks for the player and sends nothing.
    op(&mut wb, OgsKifuCommand::Search { text: "  ".into() });
    assert_eq!(view(&mut wb).status, Some(OgsKifuStatus::NeedPlayer));
    assert!(wb.take_ogs_kifu_tasks().is_empty());

    op(&mut wb, OgsKifuCommand::Search { text: " alice ".into() });
    let v = view(&mut wb);
    assert!(v.busy);
    assert_eq!(v.status, Some(OgsKifuStatus::Downloading));
    assert_eq!(v.label, OgsKifuLabel::Searched { text: "alice".into() });
    // One request at a time.
    assert_eq!(
        op(&mut wb, OgsKifuCommand::Search { text: "bob".into() }),
        CommandResult::Unchanged
    );
    let mut http = Script::default()
        .reply(
            format!("{BASE}players/?username=alice"),
            200,
            json!({"results": [{"id": 12, "username": "Alice"}]}).to_string(),
        )
        .reply(games_url(12, 1), 200, page(41, vec![game(101, "White")]));
    serve(&mut wb, &mut http);
    let v = view(&mut wb);
    assert_eq!((v.busy, v.status.clone()), (false, None));
    assert_eq!(
        v.label,
        OgsKifuLabel::Player {
            name: "Alice".into(),
            id: 12
        }
    );
    assert_eq!((v.page, v.pages, v.listed), (1, 3, true));
    assert_eq!(v.rows.len(), 1);
    assert_eq!(v.rows[0].white, "White");
    assert_eq!(v.recent, vec!["alice".to_string()]);
    let shown = v.shown;

    // |< and < do nothing on page 1; > fetches page 2 of the same player.
    assert_eq!(
        op(&mut wb, OgsKifuCommand::Page { to: KifuPage::Previous }),
        CommandResult::Unchanged
    );
    assert_eq!(
        op(&mut wb, OgsKifuCommand::Page { to: KifuPage::First }),
        CommandResult::Unchanged
    );
    op(&mut wb, OgsKifuCommand::Page { to: KifuPage::Next });
    serve(
        &mut wb,
        &mut Script::default().reply(games_url(12, 2), 200, page(41, vec![game(202, "White page 2")])),
    );
    let v = view(&mut wb);
    assert_eq!(
        (v.page, v.rows[0].white.as_str(), v.rows[0].id),
        (2, "White page 2", 202)
    );
    assert!(v.shown > shown, "a new page selects its first game");
    op(&mut wb, OgsKifuCommand::Page { to: KifuPage::Last });
    serve(
        &mut wb,
        &mut Script::default().reply(games_url(12, 3), 200, page(41, vec![game(303, "w3")])),
    );
    assert_eq!(view(&mut wb).page, 3);
    assert_eq!(
        op(&mut wb, OgsKifuCommand::Page { to: KifuPage::Next }),
        CommandResult::Unchanged
    );

    // A later search goes first; the list holds eight.
    for i in 0..9 {
        let name = format!("p{i}");
        op(&mut wb, OgsKifuCommand::Search { text: name.clone() });
        let mut http = Script::default()
            .reply(
                format!("{BASE}players/?username={name}"),
                200,
                json!({"results": [{"id": 20 + i, "username": name}]}).to_string(),
            )
            .reply(games_url(20 + i, 1), 200, page(1, vec![game(400 + i, "w")]));
        serve(&mut wb, &mut http);
    }
    let recent = view(&mut wb).recent;
    assert_eq!(recent.len(), 8);
    assert_eq!(recent[0], "p8");
    assert!(!recent.contains(&"alice".to_string()));
    // Kept in the settings for the next start.
    assert_eq!(workbench(dir.path()).view(false).ogs_kifu.recent, recent);
    op(&mut wb, OgsKifuCommand::ClearRecent);
    assert!(view(&mut wb).recent.is_empty());
}

/// A stored OGS value this version cannot read is left out on its own: an unknown 打开棋谱后此窗口
/// is no choice (野狐棋谱's applies), 近期搜索 that is no list is empty, and every other setting
/// stays without the file being put aside as unreadable.
#[test]
fn an_unreadable_ogs_setting_falls_back_alone() {
    for (field, value) in [("afterGet", json!("FutureChoice")), ("recent", json!(7))] {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data"));
        let mut config = serde_json::to_value(lizzie_core::settings::Config::default()).unwrap();
        config["foxKifu"]["afterGet"] = json!("Close");
        config["ogsKifu"] = json!({"afterGet": "Keep", "recent": ["alice"]});
        config["ogsKifu"][field] = value;
        std::fs::create_dir_all(storage.config_file.parent().unwrap()).unwrap();
        std::fs::write(&storage.config_file, serde_json::to_vec(&config).unwrap()).unwrap();
        let wb = workbench(dir.path());
        let c = wb.config();
        assert_eq!(c.fox_kifu.after_get, KifuAfterGet::Close, "{field}");
        if field == "afterGet" {
            assert_eq!((c.ogs_kifu.after_get, c.ogs_kifu.recent.clone()), (None, vec!["alice".to_string()]));
        } else {
            assert_eq!((c.ogs_kifu.after_get, c.ogs_kifu.recent.clone()), (Some(KifuAfterGet::Keep), Vec::new()));
        }
        let names: Vec<String> = std::fs::read_dir(storage.config_file.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .collect();
        assert!(!names.iter().any(|n| n.contains("unreadable-backup")), "{field}: {names:?}");
    }
}

#[test]
fn unknown_players_empty_histories_and_failures_are_told_apart() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    listed(&mut wb);
    // A failed search drops the previous player's rows at once.
    op(&mut wb, OgsKifuCommand::Search { text: "nobody".into() });
    assert!(view(&mut wb).rows.is_empty());
    serve(
        &mut wb,
        &mut Script::default().reply(
            format!("{BASE}players/?username=nobody"),
            200,
            json!({"results": []}).to_string(),
        ),
    );
    let v = view(&mut wb);
    assert_eq!(v.status, Some(OgsKifuStatus::NoPlayer));
    assert_eq!((v.listed, v.rows.len()), (false, 0));
    assert_eq!(v.label, OgsKifuLabel::Searched { text: "nobody".into() });

    op(&mut wb, OgsKifuCommand::Search { text: "7".into() });
    serve(
        &mut wb,
        &mut Script::default()
            .reply(
                format!("{BASE}players/7/"),
                200,
                json!({"id": 7, "username": "new"}).to_string(),
            )
            .reply(games_url(7, 1), 200, page(0, vec![])),
    );
    let v = view(&mut wb);
    assert_eq!(v.status, Some(OgsKifuStatus::Empty));
    assert!(v.listed);

    for (reply, failure) in [
        (Err(()), OgsFailure::Network),
        (Ok(429), OgsFailure::RateLimited),
        (Ok(502), OgsFailure::Unavailable),
    ] {
        op(&mut wb, OgsKifuCommand::Search { text: "alice".into() });
        let url = format!("{BASE}players/?username=alice");
        let mut http = match reply {
            Err(()) => Script::default().fail(url),
            Ok(status) => Script::default().reply(url, status, ""),
        };
        serve(&mut wb, &mut http);
        let v = view(&mut wb);
        assert_eq!(v.status, Some(OgsKifuStatus::Failed { failure, record: false }));
        assert!(!v.busy, "a failed search can be repeated");
    }
}

#[test]
fn an_opened_game_becomes_the_record_with_its_komi() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    listed(&mut wb);
    // Only a listed game opens.
    assert_eq!(
        op(&mut wb, OgsKifuCommand::OpenGame { id: 999 }),
        CommandResult::Unchanged
    );
    op(&mut wb, OgsKifuCommand::OpenGame { id: 102 });
    let v = view(&mut wb);
    assert!(v.busy);
    assert_eq!(v.status, Some(OgsKifuStatus::Downloading));
    // Duplicate clicks cannot start another download.
    assert_eq!(
        op(&mut wb, OgsKifuCommand::OpenGame { id: 101 }),
        CommandResult::Unchanged
    );
    serve(
        &mut wb,
        &mut Script::default().reply(format!("{BASE}games/102/sgf"), 200, SGF),
    );
    let full = wb.view(false);
    assert!(matches!(full.open_outcome, Some(OpenOutcome::Adopted { .. })));
    assert_eq!(full.komi, 6.5, "an OGS record's komi is read");
    let v = full.ogs_kifu;
    assert_eq!((v.busy, v.status.clone(), v.opened), (false, None, 1));
    let doc = wb.document();
    assert_eq!(doc.path, None);
    assert_eq!(
        (doc.record.info.black.as_str(), doc.record.info.white.as_str()),
        ("黒", "白")
    );
    assert_eq!(doc.record.node_count(), 2);
}

#[test]
fn a_failed_download_keeps_the_current_record() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    listed(&mut wb);
    let before = wb.document().id;
    for (status, failure) in [(429, OgsFailure::RateLimited), (200, OgsFailure::Unavailable)] {
        op(&mut wb, OgsKifuCommand::OpenGame { id: 101 });
        serve(
            &mut wb,
            &mut Script::default().reply(format!("{BASE}games/101/sgf"), status, "<html>Sign in</html>"),
        );
        assert_eq!(wb.document().id, before);
        assert!(wb.view(false).pending_open.is_none());
        let v = view(&mut wb);
        assert_eq!(v.status, Some(OgsKifuStatus::Failed { failure, record: true }));
        assert_eq!((v.busy, v.opened, v.rows.len()), (false, 0, 2), "the list stays");
    }
}

#[test]
fn closing_the_window_cancels_its_request_and_late_replies_change_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    listed(&mut wb);
    let recent = view(&mut wb).recent;

    // A search in flight when the window closes.
    op(&mut wb, OgsKifuCommand::Search { text: "bob".into() });
    let tasks = wb.take_ogs_kifu_tasks();
    let [OgsKifuTask::Fetch(job)] = tasks.as_slice() else {
        panic!("one request")
    };
    op(&mut wb, OgsKifuCommand::Close);
    assert_eq!(wb.take_ogs_kifu_tasks(), vec![OgsKifuTask::Cancel(job.id)]);
    let closed = wb.view(false);
    let mut http = Script::default()
        .reply(
            format!("{BASE}players/?username=bob"),
            200,
            json!({"results": [{"id": 5, "username": "bob"}]}).to_string(),
        )
        .reply(games_url(5, 1), 200, page(1, vec![game(501, "w")]));
    let late = fetch(&job.request, BASE, &mut http, false);
    assert!(!wb.ogs_kifu_reply(job, late), "a late reply changes nothing");
    let after = wb.view(false);
    assert_eq!(after.ogs_kifu, closed.ogs_kifu);
    assert_eq!(after.ogs_kifu.recent, recent);
    assert_eq!(
        (
            after.ogs_kifu.label.clone(),
            after.ogs_kifu.rows.len(),
            after.ogs_kifu.busy
        ),
        (OgsKifuLabel::Hint, 0, false)
    );

    // A download in flight: no record opens, no message, the current record stays.
    listed(&mut wb);
    let before = wb.document().id;
    op(&mut wb, OgsKifuCommand::OpenGame { id: 101 });
    let tasks = wb.take_ogs_kifu_tasks();
    let [OgsKifuTask::Fetch(job)] = tasks.as_slice() else {
        panic!("one request")
    };
    op(&mut wb, OgsKifuCommand::Close);
    assert!(wb.view(false).pending_open.is_none());
    let closed = wb.view(false);
    let late = fetch(
        &job.request,
        BASE,
        &mut Script::default().reply(format!("{BASE}games/101/sgf"), 200, SGF),
        false,
    );
    assert!(!wb.ogs_kifu_reply(job, late));
    let after = wb.view(false);
    assert_eq!(wb.document().id, before);
    assert_eq!(after.open_outcome, closed.open_outcome);
    assert_eq!(after.ogs_kifu, closed.ogs_kifu);
    assert_eq!(after.ogs_kifu.status, None);
}

/// Another open (a local file here) replacing the window's download ends it: the window is free at
/// once, the download is no longer wanted, and its late reply, success or failure, changes nothing.
#[test]
fn another_open_ends_the_windows_download_and_its_late_reply_changes_nothing() {
    for (status, body) in [(429, ""), (200, SGF)] {
        let dir = tempfile::tempdir().unwrap();
        let mut wb = workbench(dir.path());
        listed(&mut wb);
        op(&mut wb, OgsKifuCommand::OpenGame { id: 101 });
        let tasks = wb.take_ogs_kifu_tasks();
        let [OgsKifuTask::Fetch(job)] = tasks.as_slice() else {
            panic!("one request")
        };
        assert!(wb.ogs_kifu_wanted(job.id));
        assert!(open_file(&mut wb, "(;SZ[19];B[qd])"));
        let doc = wb.document().id;
        let opened = wb.view(false);
        assert_eq!(wb.take_ogs_kifu_tasks(), vec![OgsKifuTask::Cancel(job.id)]);
        assert!(!wb.ogs_kifu_wanted(job.id));
        let v = &opened.ogs_kifu;
        assert_eq!((v.busy, v.status.clone(), v.opened, v.rows.len()), (false, None, 0, 2));

        let mut http = Script::default().reply(format!("{BASE}games/101/sgf"), status, body);
        let late = fetch(&job.request, BASE, &mut http, false);
        assert!(!wb.ogs_kifu_reply(job, late), "{status}");
        let after = wb.view(false);
        assert_eq!(wb.document().id, doc);
        assert_eq!(after.open_outcome, None, "no record and no message");
        assert_eq!(after.ogs_kifu, opened.ogs_kifu);
        // The list can be used again.
        assert_eq!(
            op(&mut wb, OgsKifuCommand::Page { to: KifuPage::Next }),
            CommandResult::Done
        );
    }
}

#[test]
fn open_online_link_imports_ogs_addresses_once_and_follows_other_links_as_before() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let open = |wb: &mut Workbench, url: &str| {
        wb.online(OnlineCommand::Open {
            url: url.into(),
            refresh_seconds: 1,
        })
        .unwrap()
    };

    open(&mut wb, "https://online-go.com/game/68984609");
    assert!(wb.view(false).online.following.is_none(), "not a followed live game");
    assert!(wb.take_online_jobs().is_empty());
    serve(
        &mut wb,
        &mut Script::default().reply(format!("{BASE}games/68984609/sgf"), 200, SGF),
    );
    assert!(matches!(wb.view(false).open_outcome, Some(OpenOutcome::Adopted { .. })));
    assert_eq!(wb.document().record.info.black, "黒");
    // The window's state is not touched by a link.
    assert_eq!(wb.view(false).ogs_kifu.opened, 0);

    let before = wb.document().id;
    for (status, key) in [
        (None, OGS_LINK_FAILED),
        (Some(429), OGS_LINK_RATE_LIMITED),
        (Some(404), OGS_LINK_FAILED),
    ] {
        open(&mut wb, "https://online-go.com/api/v1/games/5/sgf/5.sgf");
        let url = format!("{BASE}games/5/sgf");
        let mut http = match status {
            None => Script::default().fail(url),
            Some(s) => Script::default().reply(url, s, ""),
        };
        serve(&mut wb, &mut http);
        let v = wb.view(false);
        assert_eq!(wb.document().id, before, "a failed link keeps the record");
        let Some(OpenOutcome::Failed { message_key, .. }) = v.open_outcome else {
            panic!("a message")
        };
        assert_eq!(message_key, key);
    }

    // Any other address takes the ordinary path: a plain link is fetched by the follow.
    open(&mut wb, "https://example.com/game/123.sgf");
    assert!(wb.take_ogs_kifu_tasks().is_empty());
    assert!(wb.view(false).online.following.is_some());
}

/// Reopening 打开在线链接 stops what it started (`Stop`): an OGS import still downloading ends
/// without a record or a message, and does not stop the link opened next.
#[test]
fn stopping_open_online_link_ends_its_ogs_import() {
    const PLAIN: &str = "https://example.com/kifu/b.sgf";
    let open = |wb: &mut Workbench, url: &str| {
        wb.online(OnlineCommand::Open {
            url: url.into(),
            refresh_seconds: 1,
        })
        .unwrap()
    };
    let import = |wb: &mut Workbench| -> OgsKifuJob {
        open(wb, "https://online-go.com/game/424242");
        let tasks = wb.take_ogs_kifu_tasks();
        let [OgsKifuTask::Fetch(job)] = tasks.as_slice() else {
            panic!("one download")
        };
        job.clone()
    };
    let late = |wb: &mut Workbench, job: &OgsKifuJob, status: u16| {
        let body = if status == 200 { SGF } else { "" };
        let mut http = Script::default().reply(format!("{BASE}games/424242/sgf"), status, body);
        let reply = fetch(&job.request, BASE, &mut http, false);
        wb.ogs_kifu_reply(job, reply)
    };

    for status in [200, 429] {
        let dir = tempfile::tempdir().unwrap();
        let mut wb = workbench(dir.path());
        let before = (wb.document().id, wb.view(false).open_outcome);
        let job = import(&mut wb);
        wb.online(OnlineCommand::Stop).unwrap();
        assert_eq!(wb.take_ogs_kifu_tasks(), vec![OgsKifuTask::Cancel(job.id)]);
        assert!(!wb.ogs_kifu_wanted(job.id));
        assert!(wb.view(false).pending_open.is_none());
        assert!(!late(&mut wb, &job, status), "{status}");
        assert_eq!((wb.document().id, wb.view(false).open_outcome), before, "{status}");
    }

    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let before = wb.document().id;
    let job = import(&mut wb);
    wb.online(OnlineCommand::Stop).unwrap();
    open(&mut wb, PLAIN);
    let jobs = wb.take_online_jobs();
    let [OnlineJob::Get { request, .. }] = jobs.as_slice() else {
        panic!("the plain link is fetched")
    };
    assert!(!late(&mut wb, &job, 200));
    assert_eq!(wb.document().id, before);
    assert_eq!(wb.view(false).online.following.map(|f| f.url).as_deref(), Some(PLAIN));
    let reply = HttpReply::Ok {
        status: 200,
        body: "(;SZ[13]PB[Plain B];B[gg])".into(),
    };
    assert!(wb.online_http(*request, reply));
    assert!(matches!(wb.view(false).open_outcome, Some(OpenOutcome::Adopted { .. })));
    assert_eq!(wb.document().record.info.black, "Plain B");
}

#[test]
fn after_open_choice_falls_back_to_fox_and_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    assert_eq!(view(&mut wb).after_get, wb.config().fox_kifu.after_get);
    op(
        &mut wb,
        OgsKifuCommand::SetAfterGet {
            action: KifuAfterGet::Keep,
        },
    );
    assert_eq!(view(&mut wb).after_get, KifuAfterGet::Keep);
    assert_eq!(
        op(
            &mut wb,
            OgsKifuCommand::SetAfterGet {
                action: KifuAfterGet::Keep
            }
        ),
        CommandResult::Unchanged
    );
    assert_eq!(
        workbench(dir.path()).config().ogs_kifu.after_get,
        Some(KifuAfterGet::Keep)
    );
}

#[test]
fn clearing_personal_data_clears_the_recent_searches() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    listed(&mut wb);
    assert!(!view(&mut wb).recent.is_empty());
    assert!(wb.clear_personal_data().unwrap());
    assert!(view(&mut wb).recent.is_empty());
}

#[test]
fn requests_are_built_from_the_api_root_only() {
    let alice = Player {
        id: 12,
        name: "alice".into(),
    };
    let mut http = Script::default().reply(
        format!("{API}players/12/games/?ended__isnull=false&ordering=-ended&page_size=20&page=2"),
        200,
        page(41, vec![]),
    );
    let Reply::Page(p) = fetch(&Request::Page { player: alice, page: 2 }, API, &mut http, false) else {
        panic!("a page")
    };
    assert_eq!((p.page, p.pages), (2, 3));
    assert!(
        http.asked
            .iter()
            .all(|u| u.starts_with("https://online-go.com/api/v1/")),
        "the server's next link is not followed"
    );
}

/// The recorded replies: `name` in `fixtures/ogs_kifu/manifest.json` gives the address it was
/// recorded from, its status and its body (checked against the recorded hash).
fn recorded(name: &str) -> (String, u16, Vec<u8>) {
    use sha2::{Digest, Sha256};
    let manifest: serde_json::Value = serde_json::from_str(include_str!("fixtures/ogs_kifu/manifest.json")).unwrap();
    let entry = manifest["requests"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == name)
        .unwrap_or_else(|| panic!("no recorded {name}"));
    let body = std::fs::read(format!("tests/fixtures/ogs_kifu/{}", entry["file"].as_str().unwrap())).unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&body)),
        entry["sha256"].as_str().unwrap(),
        "{name}"
    );
    let url = entry["url"].as_str().unwrap().to_string();
    (url, entry["status"].as_u64().unwrap() as u16, body)
}

fn replay(names: &[&str]) -> Script {
    names.iter().fold(Script::default(), |script, name| {
        let (url, status, body) = recorded(name);
        script.reply(url, status, body)
    })
}

type Row<'a> = (u64, &'a str, &'a str, &'a str, &'a str, &'a str);

fn rows(v: &OgsKifuView) -> Vec<(u64, &str, &str, &str, &str, &str)> {
    v.rows
        .iter()
        .map(|r| {
            (
                r.id,
                r.ended.as_str(),
                r.black.as_str(),
                r.white.as_str(),
                r.result.as_str(),
                r.size.as_str(),
            )
        })
        .collect()
}

/// Page 1 of the recorded player's completed games, as Java `OgsGameClient.games` reads them:
/// one winner with R, T or OGS's own outcome text, `Void` for the annulled game, `w×h`.
const RECORDED_PAGE_1: [Row; 20] = [
    (
        83665278,
        "2026-09-13T00:06:08.258245Z",
        "Crude Oil",
        "Sadaharu",
        "B+R",
        "19×19",
    ),
    (
        87483632,
        "2026-08-13T14:28:07.117894Z",
        "Crude Oil",
        "DodgerBlue",
        "W+R",
        "19×19",
    ),
    (
        84126782,
        "2026-07-26T02:37:57.697393Z",
        "做个眼",
        "Crude Oil",
        "W+16.5 points",
        "19×19",
    ),
    (
        84126791,
        "2026-06-24T03:49:23.871341Z",
        "Denosaur",
        "Crude Oil",
        "W+R",
        "19×19",
    ),
    (
        81171170,
        "2026-06-23T09:43:59.442477Z",
        "AviusZero",
        "Crude Oil",
        "W+R",
        "13×13",
    ),
    (
        84126790,
        "2026-06-20T21:01:14.059990Z",
        "Crude Oil",
        "找借口安静",
        "B+R",
        "19×19",
    ),
    (
        82110302,
        "2026-05-31T15:37:55.060836Z",
        "Crude Oil",
        "Sadaharu",
        "B+R",
        "19×19",
    ),
    (
        85372665,
        "2026-05-02T23:00:02.729493Z",
        "jeffreyfox",
        "Crude Oil",
        "W+R",
        "19×19",
    ),
    (
        84126789,
        "2026-04-28T13:06:23.938384Z",
        "vor09",
        "Crude Oil",
        "W+R",
        "19×19",
    ),
    (
        85890520,
        "2026-04-15T13:19:16.355873Z",
        "Crude Oil",
        "Lomis",
        "W+R",
        "9×9",
    ),
    (
        84126793,
        "2026-04-11T06:26:56.576839Z",
        "Crude Oil",
        "tupelo",
        "B+R",
        "19×19",
    ),
    (
        85549114,
        "2026-04-05T07:46:40.891752Z",
        "Lomis",
        "Crude Oil",
        "W+R",
        "9×9",
    ),
    (
        84126792,
        "2026-04-03T20:42:07.941615Z",
        "Crude Oil",
        "Etherion",
        "B+R",
        "19×19",
    ),
    (
        85298273,
        "2026-03-29T14:00:04.042107Z",
        "Crude Oil",
        "Lomis",
        "B+4.5 points",
        "9×9",
    ),
    (
        85006044,
        "2026-03-21T12:03:48.032534Z",
        "Lomis",
        "Crude Oil",
        "B+1.5 points",
        "9×9",
    ),
    (
        84980971,
        "2026-03-14T16:00:57.496546Z",
        "Makona79",
        "Crude Oil",
        "Void",
        "19×19",
    ),
    (
        84763726,
        "2026-03-10T01:15:13.153121Z",
        "Crude Oil",
        "Lomis",
        "B+R",
        "9×9",
    ),
    (
        83742899,
        "2026-02-26T19:23:04.251239Z",
        "Cra9Hack",
        "Crude Oil",
        "W+R",
        "19×19",
    ),
    (
        84487586,
        "2026-02-25T20:43:59.345678Z",
        "illluck",
        "Crude Oil",
        "W+T",
        "9×9",
    ),
    (
        84236136,
        "2026-02-23T14:53:51.466518Z",
        "Crude Oil",
        "Lomis",
        "W+0.5 points",
        "9×9",
    ),
];

const RECORDED_PAGE_2: [Row; 20] = [
    (
        81115501,
        "2026-02-18T02:14:54.144427Z",
        "ys7076",
        "Crude Oil",
        "W+R",
        "19×19",
    ),
    (
        84126794,
        "2026-02-12T18:51:26.166695Z",
        "Crude Oil",
        "For Our Planet",
        "Void",
        "19×19",
    ),
    (
        83912214,
        "2026-02-08T06:53:30.972783Z",
        "illluck",
        "Crude Oil",
        "B+R",
        "9×9",
    ),
    (
        83647196,
        "2026-02-05T05:00:44.582406Z",
        "illluck",
        "Crude Oil",
        "W+R",
        "9×9",
    ),
    (
        83348920,
        "2026-01-24T14:28:58.934674Z",
        "Crude Oil",
        "Lomis",
        "W+1.5 points",
        "9×9",
    ),
    (
        83037967,
        "2026-01-19T17:10:16.444661Z",
        "Lomis",
        "Crude Oil",
        "W+5.5 points",
        "9×9",
    ),
    (
        82835074,
        "2026-01-10T13:45:05.882665Z",
        "Lomis",
        "Crude Oil",
        "W+R",
        "9×9",
    ),
    (
        82562491,
        "2026-01-01T13:26:32.893861Z",
        "Lomis",
        "Crude Oil",
        "W+R",
        "9×9",
    ),
    (
        76697176,
        "2026-01-01T07:39:53.501185Z",
        "AviusZero",
        "Crude Oil",
        "W+R",
        "19×19",
    ),
    (
        82237261,
        "2025-12-22T18:41:23.013945Z",
        "Lomis",
        "Crude Oil",
        "W+0.5 points",
        "9×9",
    ),
    (
        81898403,
        "2025-12-10T22:50:48.761561Z",
        "Lomis",
        "Crude Oil",
        "W+1.5 points",
        "9×9",
    ),
    (
        81599143,
        "2025-12-03T18:28:50.331401Z",
        "Crude Oil",
        "Lomis",
        "B+0.5 points",
        "9×9",
    ),
    (
        81342364,
        "2025-12-01T15:59:55.885996Z",
        "DodgerBlue",
        "Crude Oil",
        "W+R",
        "19×19",
    ),
    (
        81336892,
        "2025-11-25T22:07:21.302643Z",
        "Crude Oil",
        "Lomis",
        "B+R",
        "9×9",
    ),
    (
        81170071,
        "2025-11-22T00:19:47.938929Z",
        "Crude Oil",
        "david.cantrell",
        "B+R",
        "19×19",
    ),
    (
        81092667,
        "2025-11-12T12:16:54.083253Z",
        "Crude Oil",
        "Lomis",
        "W+1.5 points",
        "9×9",
    ),
    (
        75818632,
        "2025-11-12T07:43:35.239033Z",
        "AviusZero",
        "Crude Oil",
        "W+R",
        "13×13",
    ),
    (
        77345592,
        "2025-11-11T12:43:38.042652Z",
        "frandekai",
        "Crude Oil",
        "W+R",
        "19×19",
    ),
    (
        80880913,
        "2025-11-05T18:32:21.313118Z",
        "illluck",
        "Crude Oil",
        "W+R",
        "9×9",
    ),
    (
        80637456,
        "2025-11-01T02:15:47.414039Z",
        "illluck",
        "Crude Oil",
        "W+1.5 points",
        "9×9",
    ),
];

/// 554 completed games, 20 a page.
const RECORDED_PAGES: u32 = 28;

/// Searches the recorded player by its username and lists page 1.
fn recorded_listed(wb: &mut Workbench) {
    op(
        wb,
        OgsKifuCommand::Search {
            text: "Crude Oil".into(),
        },
    );
    serve_at(wb, &mut replay(&["player-by-username", "games-page-1"]), API);
}

#[test]
fn recorded_replies_find_a_player_by_username_or_id_and_list_every_column() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let player = OgsKifuLabel::Player {
        name: "Crude Oil".into(),
        id: 1244057,
    };
    recorded_listed(&mut wb);
    let v = view(&mut wb);
    assert_eq!(
        (v.label.clone(), v.status.clone(), v.busy),
        (player.clone(), None, false)
    );
    assert_eq!((v.page, v.pages, v.listed), (1, RECORDED_PAGES, true));
    assert_eq!(rows(&v), RECORDED_PAGE_1);

    // The numeric ID is looked up directly and lists the same page.
    op(&mut wb, OgsKifuCommand::Search { text: "1244057".into() });
    serve_at(&mut wb, &mut replay(&["player-by-id", "games-page-1"]), API);
    let v = view(&mut wb);
    assert_eq!((v.label.clone(), v.page, v.pages), (player, 1, RECORDED_PAGES));
    assert_eq!(rows(&v), RECORDED_PAGE_1);
    assert_eq!(v.recent, vec!["1244057".to_string(), "Crude Oil".to_string()]);

    // OGS's answers for a username and an ID nobody has: 用户不存在, nothing more asked.
    for (text, name) in [
        ("zz-no-such-player-7f3c9a2e", "player-missing-username"),
        ("999999999999", "player-missing-id"),
    ] {
        op(&mut wb, OgsKifuCommand::Search { text: text.into() });
        serve_at(&mut wb, &mut replay(&[name]), API);
        let v = view(&mut wb);
        assert_eq!(v.status, Some(OgsKifuStatus::NoPlayer), "{name}");
        assert_eq!((v.listed, v.rows.len()), (false, 0), "{name}");
    }
}

#[test]
fn recorded_replies_page_to_the_next_page() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    recorded_listed(&mut wb);
    op(&mut wb, OgsKifuCommand::Page { to: KifuPage::Next });
    let mut http = replay(&["games-page-2"]);
    serve_at(&mut wb, &mut http, API);
    let v = view(&mut wb);
    assert_eq!((v.page, v.pages, v.status.clone()), (2, RECORDED_PAGES, None));
    assert_eq!(rows(&v), RECORDED_PAGE_2);
    // Asked by page number, as Java builds it, not by the reply's own `next` address.
    assert_eq!(
        http.asked,
        vec![format!(
            "{API}players/1244057/games/?ended__isnull=false&ordering=-ended&page_size=20&page=2"
        )]
    );
}

#[test]
fn a_recorded_record_opens_as_the_game_it_records() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    recorded_listed(&mut wb);
    op(&mut wb, OgsKifuCommand::OpenGame { id: 83665278 });
    serve_at(&mut wb, &mut replay(&["game-sgf"]), API);
    let full = wb.view(false);
    assert!(matches!(full.open_outcome, Some(OpenOutcome::Adopted { .. })));
    assert_eq!(full.komi, 6.5);
    assert_eq!(full.ogs_kifu.opened, 1);
    let doc = wb.document();
    assert_eq!(doc.path, None);
    let record = &doc.record;
    // The listed row's players, result and size.
    assert_eq!(
        (
            record.info.black.as_str(),
            record.info.white.as_str(),
            record.info.result.as_str()
        ),
        ("Crude Oil", "Sadaharu", "B+R")
    );
    assert_eq!((record.size.width, record.size.height), (19, 19));
    // 177 moves on the main line: B[pd] first, B[ik] last.
    let end = record.line_end(ROOT);
    assert_eq!(record.depth(end), 177);
    let first = record.main_child(ROOT).unwrap();
    assert_eq!(
        record.node(first).kind,
        NodeKind::Move {
            color: Stone::Black,
            point: Point { x: 15, y: 3 }
        }
    );
    assert_eq!(
        record.node(end).kind,
        NodeKind::Move {
            color: Stone::Black,
            point: Point { x: 8, y: 10 }
        }
    );
}

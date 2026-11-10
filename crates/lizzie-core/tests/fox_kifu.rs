//! 野狐棋谱 through the public `Workbench` operations and the Fox protocol over a scripted HTTP
//! transport: what the window shows, which requests go out, and how a downloaded record opens.

use std::collections::VecDeque;

use lizzie_core::api::{CommandResult, FoxKifuBusy, FoxKifuCommand, FoxKifuView, KifuPage, OpenOutcome, OpenSource};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::formats;
use lizzie_core::fox_kifu::fetch;
use lizzie_core::kifu_web::Http;
use lizzie_core::java_import::{self, JavaSource};
use lizzie_core::record::RecordFormat;
use lizzie_core::record::ROOT;
use lizzie_core::settings::{FoxRecentSearch, KifuAfterGet, Storage};
use lizzie_core::workbench::{FoxKifuJob, OpenOrigin, Workbench, KIFU_LOAD_FAILED};
use serde_json::json;

struct NoEngines;
impl Launcher for NoEngines {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        Err(std::io::Error::other("no engines"))
    }
}

fn storage(dir: &std::path::Path) -> Storage {
    Storage::in_dirs(&dir.join("config"), &dir.join("data"))
}

fn workbench(dir: &std::path::Path) -> Workbench {
    Workbench::new(storage(dir), None, Box::new(NoEngines), Vec::new())
}

/// Answers requests in order: each expected URL fragment with its reply.
#[derive(Default)]
struct Script {
    replies: VecDeque<(&'static str, Result<String, String>)>,
    urls: Vec<String>,
}
impl Script {
    fn reply(mut self, fragment: &'static str, body: Result<String, String>) -> Script {
        self.replies.push_back((fragment, body));
        self
    }
    fn next(&mut self, url: &str) -> Result<String, String> {
        self.urls.push(url.to_string());
        let (fragment, body) = self
            .replies
            .pop_front()
            .unwrap_or_else(|| panic!("unexpected request {url}"));
        assert!(url.contains(fragment), "{url} should contain {fragment}");
        body
    }
}
impl Http for Script {
    fn get(&mut self, url: &str, _: &str) -> Result<String, String> {
        self.next(url)
    }
    fn post_form(&mut self, url: &str, form: &[(&str, &str)], _: &str) -> Result<String, String> {
        self.next(&format!(
            "{url}?{}",
            form.iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("&")
        ))
    }
}

fn game(i: usize, black: u64, white: u64, winner: i64) -> serde_json::Value {
    json!({
        "chessid": format!("c{i}"), "starttime": format!("2026-01-01 00:00:{:02}", i % 60),
        "blackuid": black, "blacknick": format!("b{black}"), "blackdan": 108,
        "whiteuid": white, "whitenick": format!("w{white}"), "whitedan": 20,
        "movenum": 200, "winner": winner, "point": -1, "rule": 0,
    })
}

fn list(range: std::ops::Range<usize>) -> Result<String, String> {
    let games: Vec<_> = range
        .map(|i| game(i, 100, 200 + i as u64, if i % 2 == 0 { 1 } else { 2 }))
        .collect();
    Ok(json!({"result": 0, "chesslist": games}).to_string())
}

fn account(uid: &str, name: &str) -> Result<String, String> {
    Ok(json!({"result": 0, "uid": uid, "username": name}).to_string())
}

/// Runs the queued requests against `http`, in order, and hands each reply back.
fn serve(wb: &mut Workbench, http: &mut Script) -> Vec<FoxKifuJob> {
    let jobs = wb.take_fox_kifu_jobs();
    for job in &jobs {
        let reply = fetch(&job.request, http, job.no_capture);
        wb.fox_kifu_reply(job, reply);
    }
    jobs
}

fn op(wb: &mut Workbench, op: FoxKifuCommand) -> CommandResult {
    wb.fox_kifu(op).unwrap()
}

fn search(wb: &mut Workbench, text: &str, by_uid: bool) -> CommandResult {
    op(
        wb,
        FoxKifuCommand::Search {
            text: text.into(),
            by_uid,
            nickname_hint: String::new(),
        },
    )
}

fn view(wb: &mut Workbench) -> FoxKifuView {
    wb.view(false).fox_kifu
}

fn refused(key: &str) -> CommandResult {
    CommandResult::Refused {
        message_key: key.into(),
    }
}

#[test]
fn a_nickname_search_resolves_the_account_then_lists_its_games() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    op(&mut wb, FoxKifuCommand::Open);
    // An all-digit nickname is still looked up as a nickname.
    search(&mut wb, " 00123 ", false);
    let v = view(&mut wb);
    assert_eq!(v.busy, Some(FoxKifuBusy::Searching { text: "00123".into() }));
    assert_eq!((v.nickname.as_str(), v.uid.as_str()), ("00123", ""));
    let mut http = Script::default()
        .reply("QueryUserInfoPanel?srcuid=0&username=00123", account("100", "Kj"))
        .reply("dstuid=100&type=1&lastcode=0", list(0..30));
    serve(&mut wb, &mut http);
    let v = view(&mut wb);
    assert_eq!(v.busy, None);
    // The searched account's own nickname from its games, with its UID.
    assert_eq!((v.nickname.as_str(), v.uid.as_str()), ("b100", "100"));
    assert_eq!(
        v.recent,
        vec![FoxRecentSearch {
            uid: "100".into(),
            nickname: "b100".into()
        }]
    );
    assert_eq!((v.page, v.pages, v.complete, v.listed), (1, 2, true, true));
    assert_eq!(v.rows.len(), 25);
    assert_eq!(v.rows[0].index, 1);
    // Black (the account) won game 0; White won game 1.
    assert!(v.rows[0].won && !v.rows[1].won);
    // Remembered for the next start.
    let reopened = workbench(dir.path());
    assert_eq!(reopened.config().fox_kifu.last_name, "00123");
    assert!(!reopened.config().fox_kifu.last_query_by_uid);
}

#[test]
fn missing_text_a_non_numeric_uid_and_a_running_search_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    assert_eq!(search(&mut wb, "  ", false), refused("FoxKifuDownload.noUser"));
    assert_eq!(search(&mut wb, "", true), refused("FoxKifuDownload.noUid"));
    assert_eq!(search(&mut wb, "12a", true), refused("FoxKifuDownload.invalidUidInput"));
    assert!(wb.take_fox_kifu_jobs().is_empty());
    search(&mut wb, "abc", false);
    assert_eq!(search(&mut wb, "def", false), refused("FoxKifuDownload.waitLastSearch"));
}

#[test]
fn an_unknown_account_and_a_transport_failure_say_different_things() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    search(&mut wb, "nobody", false);
    let mut http = Script::default().reply(
        "QueryUserInfoPanel",
        Ok(json!({"result": 101201, "uid": "0", "username": ""}).to_string()),
    );
    serve(&mut wb, &mut http);
    assert_eq!(http.urls.len(), 1, "no games are fetched for a missing account");
    let n = view(&mut wb).notice.unwrap();
    assert_eq!(n.message_key, "FoxKifuDownload.invalidUid");

    search(&mut wb, "someone", false);
    let mut http = Script::default().reply("QueryUserInfoPanel", Err("connect timed out".into()));
    serve(&mut wb, &mut http);
    let v = view(&mut wb);
    let failure = v.notice.unwrap();
    assert!(failure.id > n.id);
    assert_eq!(failure.message_key, "FoxKifuDownload.getKifuFailed");
    assert_eq!(failure.detail, "connect timed out");
    assert_eq!(v.busy, None);
    // The failed search is over: another may start.
    assert_eq!(search(&mut wb, "again", false), CommandResult::Done);
}

#[test]
fn an_account_without_games_says_so_and_lists_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    listed(&mut wb);
    search(&mut wb, "300", true);
    let mut http = Script::default().reply("dstuid=300", Ok(json!({"result": 0, "chesslist": []}).to_string()));
    serve(&mut wb, &mut http);
    let v = view(&mut wb);
    assert_eq!(v.notice.unwrap().message_key, "FoxKifuDownload.noKifu");
    assert_eq!(v.busy, None);
    assert!(!v.listed, "the page buttons have no list to page through");
    assert_eq!(
        op(&mut wb, FoxKifuCommand::OpenGame { chessid: "c0".into() }),
        CommandResult::Done,
        "the shown rows of the previous search still open, as Java's table keeps them"
    );
    assert_eq!(search(&mut wb, "again", false), refused("KifuLoad.wait"));
}

#[test]
fn a_replaced_search_never_reaches_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    search(&mut wb, "1", true);
    let old = wb.take_fox_kifu_jobs();
    // A failed first search lets the next one start; the first one's late list must not land.
    let mut http = Script::default().reply("dstuid=1&", Err("timeout".into()));
    wb.fox_kifu_reply(&old[0], fetch(&old[0].request, &mut http, false));
    search(&mut wb, "2", true);
    let mut late = Script::default().reply("dstuid=1&", list(0..5));
    assert!(!wb.fox_kifu_reply(&old[0], fetch(&old[0].request, &mut late, false)));
    let mut http = Script::default().reply("dstuid=2&", list(0..3));
    serve(&mut wb, &mut http);
    let v = view(&mut wb);
    assert_eq!(v.uid, "2");
    assert_eq!(v.rows.len(), 3);
}

#[test]
fn the_last_pages_fetch_more_and_a_failed_page_can_be_asked_again() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    search(&mut wb, "100", true);
    let mut http = Script::default().reply("dstuid=100&type=1&lastcode=0", list(0..100));
    serve(&mut wb, &mut http);
    let v = view(&mut wb);
    assert_eq!((v.page, v.pages, v.complete), (1, 4, false));
    // Page 2 is not near the end: nothing is fetched.
    op(&mut wb, FoxKifuCommand::Page { to: KifuPage::Next });
    assert!(wb.take_fox_kifu_jobs().is_empty());
    // Page 3 of 4 asks for the games after the last one.
    op(&mut wb, FoxKifuCommand::Page { to: KifuPage::Next });
    let mut failing = Script::default().reply("lastcode=c99", Err("reset".into()));
    serve(&mut wb, &mut failing);
    assert_eq!(
        view(&mut wb).notice.unwrap().message_key,
        "FoxKifuDownload.getKifuFailed"
    );
    // The failed page is requested again, and its games extend the list.
    op(&mut wb, FoxKifuCommand::Page { to: KifuPage::Next });
    let v = view(&mut wb);
    assert_eq!((v.page, v.pages), (4, 4));
    let mut http = Script::default().reply("lastcode=c99", list(100..130));
    serve(&mut wb, &mut http);
    let v = view(&mut wb);
    // Asked from the last page: the list moves on to the next page.
    assert_eq!((v.page, v.pages, v.complete), (5, 6, true));
    assert_eq!(v.rows[0].index, 101);
    op(&mut wb, FoxKifuCommand::Page { to: KifuPage::Last });
    let mut http = Script::default().reply("lastcode=c129", Ok(json!({"result": 0, "chesslist": []}).to_string()));
    serve(&mut wb, &mut http);
    // No more games: the second try at the end says so.
    assert_eq!(
        op(&mut wb, FoxKifuCommand::Page { to: KifuPage::Next }),
        refused("FoxKifuDownload.noMoreKifu")
    );
    op(&mut wb, FoxKifuCommand::Page { to: KifuPage::First });
    assert_eq!(view(&mut wb).rows[0].index, 1);
}

fn listed(wb: &mut Workbench) {
    search(wb, "100", true);
    let mut http = Script::default().reply("dstuid=100", list(0..3));
    serve(wb, &mut http);
}

#[test]
fn a_downloaded_record_opens_through_the_record_reader() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    listed(&mut wb);
    let before = wb.document().id;
    op(&mut wb, FoxKifuCommand::OpenGame { chessid: "c1".into() });
    let v = view(&mut wb);
    assert!(v.loading);
    assert_eq!(v.busy, Some(FoxKifuBusy::Downloading));
    // While it downloads, another game and another search wait.
    assert_eq!(
        op(&mut wb, FoxKifuCommand::OpenGame { chessid: "c2".into() }),
        CommandResult::Unchanged
    );
    assert_eq!(search(&mut wb, "x", false), refused("KifuLoad.wait"));
    // The first record CGI has no record; the second has it, with Fox's variation layout.
    let sgf = "(;GM[1]FF[4]SZ[19]PB[b100]PW[w201]KM[650];B[pd](;W[dp];B[qp])(;W[dd]))";
    let mut http = Script::default()
        .reply(
            "TXWQFetchChess?chessid=c1",
            Ok(json!({"result": -3, "resultstr": "none"}).to_string()),
        )
        .reply(
            "TXWQFetchChess?chessid=c1",
            Ok(json!({"result": 0, "chess": sgf}).to_string()),
        );
    serve(&mut wb, &mut http);
    let full = wb.view(false);
    assert!(matches!(full.open_outcome, Some(OpenOutcome::Adopted { .. })));
    let v = full.fox_kifu;
    assert!(!v.loading);
    assert_eq!(v.busy, None);
    assert_eq!(v.opened, 1);
    let doc = wb.document();
    assert_ne!(doc.id, before);
    assert_eq!(doc.path, None, "a downloaded record has no file of its own");
    // Only the main line remains (GetFoxRequest.retainMainLineOnly).
    let rec = &doc.record;
    let first = rec.node(ROOT).children.clone();
    assert_eq!(first.len(), 1);
    let second = rec.node(first[0]).children.clone();
    assert_eq!(second.len(), 1);
    assert_eq!(rec.info.black, "b100");
}

#[test]
fn an_unreadable_record_keeps_the_open_one_and_ends_the_download() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    listed(&mut wb);
    let before = wb.document().id;
    op(&mut wb, FoxKifuCommand::OpenGame { chessid: "c0".into() });
    let mut http = Script::default()
        .reply("happyapp", Err("down".into()))
        .reply("cgi.foxwq.com", Err("down".into()))
        .reply(
            "YHWQFetchChess?chessid=c0",
            Ok(json!({"result": 0, "chess": "(;C[x"}).to_string()),
        );
    serve(&mut wb, &mut http);
    let full = wb.view(false);
    assert!(
        matches!(full.open_outcome, Some(OpenOutcome::Failed { ref message_key, .. }) if message_key == KIFU_LOAD_FAILED)
    );
    assert_eq!(wb.document().id, before);
    let v = view(&mut wb);
    assert!(!v.loading);
    assert_eq!(v.opened, 0);
    // A Fox error reply is shown with its own text.
    op(&mut wb, FoxKifuCommand::OpenGame { chessid: "c0".into() });
    let mut http = Script::default()
        .reply("happyapp", Err("down".into()))
        .reply("cgi.foxwq.com", Err("down".into()))
        .reply(
            "YHWQFetchChess",
            Ok(json!({"result": 5, "resultstr": "busy"}).to_string()),
        );
    serve(&mut wb, &mut http);
    let v = view(&mut wb);
    assert!(!v.loading);
    assert_eq!(v.notice.unwrap().detail, "busy");
}

/// The record reply of a download that was taken off the queue before another open started.
fn deliver_record(wb: &mut Workbench, jobs: &[FoxKifuJob]) {
    let sgf = "(;GM[1]FF[4]SZ[19]PB[fox]PW[w];B[pd];W[dp])";
    let mut http = Script::default().reply("happyapp", Ok(json!({"result": 0, "chess": sgf}).to_string()));
    for job in jobs {
        let reply = fetch(&job.request, &mut http, job.no_capture);
        wb.fox_kifu_reply(job, reply);
    }
}

fn local_record() -> Result<(RecordFormat, lizzie_core::record::GameRecord), lizzie_core::formats::ReadError> {
    let rec = formats::parse(RecordFormat::Sgf, "(;GM[1]FF[4]SZ[19]PB[local];B[aa])", false)?;
    Ok((RecordFormat::Sgf, rec))
}

fn open_file(wb: &mut Workbench) -> u64 {
    wb.begin_open(OpenOrigin::File {
        path: "/records/local.sgf".into(),
        source: OpenSource::Chooser,
    })
}

#[test]
fn a_record_opened_while_a_game_downloads_is_not_replaced_by_it() {
    // Read still running when the download ends: the read stays the one the user waits for.
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    listed(&mut wb);
    op(&mut wb, FoxKifuCommand::OpenGame { chessid: "c1".into() });
    let download = wb.take_fox_kifu_jobs();
    let reading = open_file(&mut wb);
    let v = view(&mut wb);
    assert!(!v.loading, "the download ends when another open replaces it");
    assert_eq!(v.busy, None);
    deliver_record(&mut wb, &download);
    assert!(
        wb.is_current_open(reading),
        "a late download must not supersede the read"
    );
    assert!(wb.complete_open(reading, local_record()));
    assert_eq!(wb.document().record.info.black, "local");
    assert_eq!(view(&mut wb).opened, 0);

    // Read already adopted when the download ends: the record stays.
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    listed(&mut wb);
    op(&mut wb, FoxKifuCommand::OpenGame { chessid: "c1".into() });
    let download = wb.take_fox_kifu_jobs();
    let reading = open_file(&mut wb);
    assert!(wb.complete_open(reading, local_record()));
    let opened = wb.document().id;
    deliver_record(&mut wb, &download);
    let full = wb.view(false);
    assert_eq!(
        wb.document().id,
        opened,
        "a late download must not replace the opened record"
    );
    assert_eq!(wb.document().record.info.black, "local");
    assert!(full.pending_open.is_none());
    let v = full.fox_kifu;
    assert!(!v.loading);
    assert_eq!(v.busy, None);
    assert_eq!(
        v.opened, 0,
        "打开棋谱后此窗口 must not act for a download that was not opened"
    );
    // The window takes a new download again.
    op(&mut wb, FoxKifuCommand::OpenGame { chessid: "c2".into() });
    assert!(view(&mut wb).loading);
}

#[test]
fn window_choices_persist_and_personal_data_clears_recent_searches() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    listed(&mut wb);
    op(
        &mut wb,
        FoxKifuCommand::SetAfterGet {
            action: KifuAfterGet::Keep,
        },
    );
    assert_eq!(workbench(dir.path()).config().fox_kifu.after_get, KifuAfterGet::Keep);
    assert_eq!(view(&mut wb).recent.len(), 1);
    assert!(wb.clear_personal_data().unwrap());
    assert!(view(&mut wb).recent.is_empty());
    assert!(workbench(dir.path()).config().fox_kifu.recent.is_empty());
    listed(&mut wb);
    op(&mut wb, FoxKifuCommand::ClearRecent);
    assert!(view(&mut wb).recent.is_empty());
}

#[test]
fn recent_searches_keep_eight_newest_accounts_first() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    for uid in 1..=9 {
        search(&mut wb, &uid.to_string(), true);
        let games = json!({"result": 0, "chesslist": [game(0, uid, 999, 1)]}).to_string();
        let mut http = Script::default().reply("dstuid=", Ok(games));
        serve(&mut wb, &mut http);
    }
    search(&mut wb, "3", true);
    let games = json!({"result": 0, "chesslist": [game(0, 3, 999, 1)]}).to_string();
    serve(&mut wb, &mut Script::default().reply("dstuid=3", Ok(games)));
    let recent = view(&mut wb).recent;
    let uids: Vec<_> = recent.iter().map(|r| r.uid.as_str()).collect();
    assert_eq!(uids, ["3", "9", "8", "7", "6", "5", "4", "2"]);
}

#[test]
fn java_fox_settings_are_imported() {
    let dir = tempfile::tempdir().unwrap();
    let java = dir.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let config = json!({"ui": {
        "last-fox-name": "6757425", "last-fox-query-by-uid": true, "fox-after-get": 1,
        "fox-recent-searches": [{"uid": "6757425", "nickname": "柯洁"}, {"uid": "", "nickname": ""}, {"nickname": "abc"}],
    }});
    std::fs::write(java.join("config.txt"), config.to_string()).unwrap();
    let candidate = java_import::read(&JavaSource::new(&java, None)).unwrap();
    let wb = workbench(dir.path());
    let (c, _) = candidate.mapped.apply(wb.config(), wb.persisted(), &[], false);
    assert_eq!(c.fox_kifu.last_name, "6757425");
    assert!(c.fox_kifu.last_query_by_uid);
    assert_eq!(c.fox_kifu.after_get, KifuAfterGet::Close);
    assert_eq!(
        c.fox_kifu.recent,
        vec![
            FoxRecentSearch {
                uid: "6757425".into(),
                nickname: "柯洁".into()
            },
            FoxRecentSearch {
                uid: String::new(),
                nickname: "abc".into()
            },
        ]
    );
}

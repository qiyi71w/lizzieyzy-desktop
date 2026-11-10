//! 腾讯棋谱 through the public workbench interface with a scripted HTTP transport: the searches,
//! list and paging of `TencentKifuDownload`, record downloads through the ordinary open path, and
//! the Java settings it keeps. Fixtures under `fixtures/tencent_kifu` are real service replies.

use std::collections::VecDeque;

use lizzie_core::api::{
    CommandResult, ImportStatus, ImportTarget, KifuPage, OpenOutcome, OpenSource, TencentKifuBusy, TencentKifuCommand,
    TencentKifuView,
};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::java_import::{self, JavaSource};
use lizzie_core::kifu_web::Http;
use lizzie_core::record::ROOT;
use lizzie_core::settings::{KifuAfterGet, Storage, TencentRecentSearch};
use lizzie_core::tencent_kifu::{fetch, sanitize};
use lizzie_core::workbench::{OpenOrigin, TencentKifuJob, Workbench, KIFU_LOAD_FAILED};
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

/// Answers requests in order: each expected URL fragment with its reply.
#[derive(Default)]
struct Script {
    replies: VecDeque<(&'static str, Result<String, String>)>,
}
impl Script {
    fn reply(mut self, fragment: &'static str, body: Result<String, String>) -> Script {
        self.replies.push_back((fragment, body));
        self
    }
}
impl Http for Script {
    fn get(&mut self, url: &str, _: &str) -> Result<String, String> {
        let (fragment, body) = self
            .replies
            .pop_front()
            .unwrap_or_else(|| panic!("unexpected request {url}"));
        assert!(url.contains(fragment), "{url} should contain {fragment}");
        body
    }
    fn post_form(&mut self, url: &str, _: &[(&str, &str)], _: &str) -> Result<String, String> {
        panic!("the Tencent service is only read with GET: {url}")
    }
}

fn game(i: usize, black: &str, white: &str, winner: i64) -> serde_json::Value {
    json!({
        "chessid": format!("c{i}"), "starttime": format!("2026-01-01 00:00:{:02}", i % 60),
        "blackuid": 100 + i, "blacknick": black, "blackdan": 108,
        "whiteuid": 200 + i, "whitenick": white, "whitedan": 20,
        "movenum": 200, "winner": winner, "point": 0, "rule": 2,
    })
}

fn list(range: std::ops::Range<usize>) -> Result<String, String> {
    let games: Vec<_> = range.map(|i| game(i, "KeJie", &format!("w{i}"), 1 + (i % 2) as i64)).collect();
    Ok(json!({"result": 0, "ret": 0, "chesslist": games}).to_string())
}

fn serve(wb: &mut Workbench, http: &mut Script) -> Vec<TencentKifuJob> {
    let jobs = wb.take_tencent_kifu_jobs();
    for job in &jobs {
        let reply = fetch(&job.request, http, job.no_capture);
        wb.tencent_kifu_reply(job, reply);
    }
    jobs
}

fn op(wb: &mut Workbench, op: TencentKifuCommand) -> CommandResult {
    wb.tencent_kifu(op).unwrap()
}

fn search(wb: &mut Workbench, text: &str) -> CommandResult {
    op(wb, TencentKifuCommand::Search { text: text.into() })
}

fn view(wb: &mut Workbench) -> TencentKifuView {
    wb.view(false).tencent_kifu
}

fn refused(key: &str) -> CommandResult {
    CommandResult::Refused {
        message_key: key.into(),
    }
}

fn notice(wb: &mut Workbench) -> (String, String) {
    let n = view(wb).notice.expect("a message box");
    (n.message_key, n.detail)
}

#[test]
fn a_search_lists_the_games_of_a_nickname_or_uid() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    op(&mut wb, TencentKifuCommand::Open);
    assert_eq!(view(&mut wb).query, "");
    assert_eq!(search(&mut wb, "  "), refused("TencentKifuDownload.noUser"));
    search(&mut wb, " KeJie ");
    let v = view(&mut wb);
    assert_eq!(v.busy, Some(TencentKifuBusy::Searching { text: "KeJie".into() }));
    assert_eq!((v.query.as_str(), v.last_name.as_str()), ("KeJie", "KeJie"));
    // Another search waits for this one.
    assert_eq!(search(&mut wb, "x"), refused("TencentKifuDownload.waitLastSearch"));
    // A nickname is not a source UID; the whole service request is Java's.
    let mut http = Script::default().reply(
        "TXWQFetchChessList?type=7&lastCode=0&username=KeJie&srcuid=&txwqsession=lizzieyzy-next&fetchnum=100",
        list(0..30),
    );
    serve(&mut wb, &mut http);
    let v = view(&mut wb);
    assert_eq!(v.busy, None);
    assert_eq!(v.rows.len(), 25);
    assert_eq!((v.page, v.pages, v.complete), (1, 2, true), "fewer than 100 games: the list is complete");
    // The searched nickname wins as Black (letter case aside), not as White.
    assert!(v.rows[0].won && !v.rows[1].won);
    assert_eq!(v.recent, vec![TencentRecentSearch { uid: String::new(), query: "KeJie".into() }]);

    // An all-digit text is also the source UID, and wins are then matched by UID.
    search(&mut wb, "201");
    let games = json!({"result": 0, "chesslist": [game(1, "a", "b", 2), game(2, "c", "d", 1)]}).to_string();
    serve(
        &mut wb,
        &mut Script::default().reply("username=201&srcuid=201&", Ok(games)),
    );
    let v = view(&mut wb);
    assert_eq!(v.rows.len(), 2);
    assert!(v.rows[0].won, "white uid 201 won game 1");
    assert!(!v.rows[1].won);
    assert_eq!(v.recent[0], TencentRecentSearch { uid: "201".into(), query: "201".into() });
}

#[test]
fn a_real_service_list_reads_like_java() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    search(&mut wb, "KeJie");
    let body = std::fs::read_to_string("tests/fixtures/tencent_kifu/list.json").unwrap();
    serve(&mut wb, &mut Script::default().reply("username=KeJie", Ok(body)));
    let v = view(&mut wb);
    assert_eq!((v.page, v.pages, v.complete), (1, 2, true));
    let first = &v.rows[0];
    assert_eq!(first.chessid, "1793582658010001015");
    assert_eq!(first.time, "2026-11-02 09:24:18");
    assert_eq!((first.black_name.as_deref(), first.black_uid.as_str()), (Some("柯洁"), "6757425"));
    assert_eq!((first.black_rank, first.white_rank), (108, 108));
    assert_eq!((first.winner, first.point, first.rule, first.moves), (2, 0, 2, 252));
    // KeJie is 柯洁's English name: Java compares the shown nickname, so no win is marked.
    assert!(v.rows.iter().all(|r| !r.won));
}

#[test]
fn failures_are_reported_and_leave_the_search_usable() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    for (body, detail) in [
        (Err("timed out".to_string()), "timed out"),
        (Ok(json!({"result": -3, "resultstr": "busy"}).to_string()), "busy"),
        (Ok(json!({"result": 0, "ret": 7}).to_string()), r#"{"result":0,"ret":7}"#),
        // Java waited forever on these two; they fail here.
        (Ok(String::new()), "empty response from server"),
        (Ok(json!({"result": 0}).to_string()), r#"{"result":0}"#),
    ] {
        search(&mut wb, "KeJie");
        serve(&mut wb, &mut Script::default().reply("username=KeJie", body));
        let v = view(&mut wb);
        assert_eq!(v.busy, None, "{detail}");
        assert_eq!(
            notice(&mut wb),
            ("TencentKifuDownload.getKifuFailed".into(), detail.into())
        );
    }
    // No games at all.
    search(&mut wb, "nobody");
    let empty = json!({"result": 0, "chesslist": []}).to_string();
    serve(&mut wb, &mut Script::default().reply("username=nobody", Ok(empty)));
    assert_eq!(notice(&mut wb).0, "TencentKifuDownload.noKifu");
    assert!(!view(&mut wb).listed);
    assert!(view(&mut wb).recent.is_empty(), "a search without games is not remembered");
}

#[test]
fn a_reply_of_a_replaced_search_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    search(&mut wb, "old");
    let old = wb.take_tencent_kifu_jobs();
    // The search failed before its reply came back; a new one starts.
    let failed = lizzie_core::tencent_kifu::Reply::Failed { message: "x".into() };
    assert!(wb.tencent_kifu_reply(&old[0], failed));
    search(&mut wb, "new");
    let reply = fetch(&old[0].request, &mut Script::default().reply("username=old", list(0..3)), false);
    assert!(!wb.tencent_kifu_reply(&old[0], reply));
    assert!(!view(&mut wb).listed);
    assert_eq!(view(&mut wb).busy, Some(TencentKifuBusy::Searching { text: "new".into() }));
}

#[test]
fn the_last_pages_fetch_more_and_a_failed_list_can_be_asked_again() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    search(&mut wb, "KeJie");
    serve(&mut wb, &mut Script::default().reply("lastCode=0&", list(0..100)));
    let v = view(&mut wb);
    assert_eq!((v.page, v.pages, v.complete), (1, 4, false));
    op(&mut wb, TencentKifuCommand::Page { to: KifuPage::Next });
    assert!(wb.take_tencent_kifu_jobs().is_empty(), "page 2 of 4 fetches nothing");
    // Page 3 of 4 asks for the games after the last one, under a progress card.
    op(&mut wb, TencentKifuCommand::Page { to: KifuPage::Next });
    assert_eq!(view(&mut wb).busy, Some(TencentKifuBusy::LoadingMore));
    serve(&mut wb, &mut Script::default().reply("lastCode=c99&", Err("reset".into())));
    let v = view(&mut wb);
    assert_eq!(v.busy, None);
    assert_eq!(v.notice.unwrap().detail, "reset");
    // The failed list is requested again (Java kept the failed code and never asked again).
    op(&mut wb, TencentKifuCommand::Page { to: KifuPage::Next });
    serve(&mut wb, &mut Script::default().reply("lastCode=c99&", list(100..130)));
    let v = view(&mut wb);
    assert_eq!((v.page, v.pages, v.complete), (5, 6, true), "the last page moved on to the new games");
    assert_eq!(v.rows[0].index, 101);
    op(&mut wb, TencentKifuCommand::Page { to: KifuPage::Last });
    serve(
        &mut wb,
        &mut Script::default().reply("lastCode=c129&", Ok(json!({"result": 0, "chesslist": []}).to_string())),
    );
    // No more games: the second try at the end says so.
    assert_eq!(
        op(&mut wb, TencentKifuCommand::Page { to: KifuPage::Next }),
        refused("TencentKifuDownload.noMoreKifu")
    );
    op(&mut wb, TencentKifuCommand::Page { to: KifuPage::First });
    assert_eq!(view(&mut wb).rows[0].index, 1);
}

fn listed(wb: &mut Workbench) {
    search(wb, "KeJie");
    serve(wb, &mut Script::default().reply("username=KeJie", list(0..3)));
}

fn download(wb: &mut Workbench, chessid: &'static str, body: Result<String, String>) {
    op(wb, TencentKifuCommand::OpenGame { chessid: chessid.into() });
    let mut http = Script::default().reply("happyapp.huanle.qq.com/cgi-bin/CommonMobileCGI/TXWQFetchChess?chessid=", body);
    serve(wb, &mut http);
}

#[test]
fn a_real_record_opens_with_its_variations_and_comments_and_the_session_komi() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    assert!(wb.config().record.read_komi, "automatic komi loading is on");
    let komi = wb.view(false).komi;
    listed(&mut wb);
    op(&mut wb, TencentKifuCommand::OpenGame { chessid: "c1".into() });
    let v = view(&mut wb);
    assert!(v.loading);
    assert_eq!(v.busy, Some(TencentKifuBusy::Downloading));
    assert_eq!(op(&mut wb, TencentKifuCommand::OpenGame { chessid: "c2".into() }), CommandResult::Unchanged);
    assert_eq!(search(&mut wb, "x"), refused("KifuLoad.wait"));
    let body = std::fs::read_to_string("tests/fixtures/tencent_kifu/record.json").unwrap();
    serve(
        &mut wb,
        &mut Script::default().reply("TXWQFetchChess?chessid=c1", Ok(body)),
    );
    let full = wb.view(false);
    assert!(matches!(full.open_outcome, Some(OpenOutcome::Adopted { .. })));
    // KM[650] (komi x 100) is not taken: Java reads Tencent records with komi loading off.
    assert_eq!(full.komi, komi);
    let v = full.tencent_kifu;
    assert!(!v.loading);
    assert_eq!(v.busy, None);
    assert_eq!(v.opened, 1);
    let doc = wb.document();
    assert_eq!(doc.path, None, "a downloaded record has no file of its own");
    let rec = &doc.record;
    assert_eq!((rec.info.black.as_str(), rec.info.white.as_str()), ("柯洁", "江维杰"));
    // Every variation and comment of the service record stays (6929 move nodes and 758 nodes with
    // a C property, counted by an independent SGF property parse of the reply).
    assert_eq!(rec.node_count(), 1 + 6929);
    let commented = (0..rec.node_count() as u32).filter(|&id| !rec.node(id).comment.is_empty()).count();
    assert_eq!(commented, 758);
    let (mut node, mut depth) = (ROOT, 0);
    while let Some(&first) = rec.node(node).children.first() {
        node = first;
        depth += 1;
    }
    assert_eq!(depth, 252, "the main line is the listed 252 moves");
    assert!(rec.node(node).comment.starts_with("应该是认输了"));
}

#[test]
fn a_handicap_record_keeps_its_komi_and_stray_backslashes_are_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    listed(&mut wb);
    let sgf = "\u{feff} (;GM[1]SZ[19]HA[2]AB[dd][pp]KM[0.5]\\\n;W[qd]C[a\\]b\\\\c];B[dp]) ";
    download(&mut wb, "c0", Ok(json!({"result": 0, "chess": sgf}).to_string()));
    assert_eq!(wb.view(false).komi, 0.5, "setup and handicap records take their komi");
    let rec = &wb.document().record;
    let first = rec.node(ROOT).children[0];
    assert_eq!(rec.node(first).comment, "a]b\\c");
    assert_eq!(rec.node(rec.node(first).children[0]).children.len(), 0);
    // The clean-up alone (sanitizeTencentSgf).
    assert_eq!(sanitize(sgf), "(;GM[1]SZ[19]HA[2]AB[dd][pp]KM[0.5]\n;W[qd]C[a\\]b\\\\c];B[dp])");
}

#[test]
fn a_failed_download_keeps_the_open_record() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    listed(&mut wb);
    let before = wb.document().id;
    // The service's reply for an unknown game.
    let missing = json!({"result": -3, "resultstr": "FetchChessFromDB Failed!!", "uin": 0}).to_string();
    download(&mut wb, "c0", Ok(missing));
    assert_eq!(wb.document().id, before);
    assert!(wb.view(false).pending_open.is_none());
    let v = view(&mut wb);
    assert!(!v.loading && v.busy.is_none());
    assert_eq!(
        v.notice.unwrap(),
        lizzie_core::api::KifuNotice {
            id: 1,
            message_key: "TencentKifuDownload.getKifuFailed".into(),
            detail: "FetchChessFromDB Failed!!".into()
        }
    );
    // An unreadable record fails in the main window, as any downloaded record.
    download(&mut wb, "c0", Ok(json!({"result": 0, "chess": "(;C[x"}).to_string()));
    let full = wb.view(false);
    assert!(
        matches!(full.open_outcome, Some(OpenOutcome::Failed { ref message_key, .. }) if message_key == KIFU_LOAD_FAILED)
    );
    assert_eq!(wb.document().id, before);
    assert!(!view(&mut wb).loading);
    // Text that is no record reply is shown as it came.
    download(&mut wb, "c0", Ok("<html>gone</html>".into()));
    assert_eq!(notice(&mut wb).1, "<html>gone</html>");
    assert_eq!(view(&mut wb).opened, 0);
}

#[test]
fn a_record_opened_while_a_game_downloads_is_not_replaced_by_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    listed(&mut wb);
    op(&mut wb, TencentKifuCommand::OpenGame { chessid: "c1".into() });
    let downloading = wb.take_tencent_kifu_jobs();
    let reading = wb.begin_open(OpenOrigin::File {
        path: "/records/local.sgf".into(),
        source: OpenSource::Chooser,
    });
    let v = view(&mut wb);
    assert!(!v.loading, "the download ends when another open replaces it");
    let sgf = "(;GM[1]SZ[19]PB[tencent];B[pd])";
    let mut http = Script::default().reply("chessid=c1", Ok(json!({"result": 0, "chess": sgf}).to_string()));
    let reply = fetch(&downloading[0].request, &mut http, false);
    assert!(!wb.tencent_kifu_reply(&downloading[0], reply));
    assert!(wb.is_current_open(reading), "a late download must not supersede the read");
    assert_eq!(view(&mut wb).opened, 0);
}

#[test]
fn the_window_takes_fox_s_after_open_choice_when_it_first_opens() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let fox = |wb: &mut Workbench, action| {
        wb.fox_kifu(lizzie_core::api::FoxKifuCommand::SetAfterGet { action }).unwrap();
    };
    // Before the window first opens, 野狐棋谱's choice is the one it would take.
    fox(&mut wb, KifuAfterGet::Close);
    assert_eq!(view(&mut wb).after_get, KifuAfterGet::Close);
    op(&mut wb, TencentKifuCommand::Open);
    // TencentKifuDownload reads `optInt("tencent-after-get", foxAfterGet)` once, when the frame
    // is built; a later 野狐棋谱 choice does not reach the open window, also after hiding it.
    fox(&mut wb, KifuAfterGet::Keep);
    assert_eq!(view(&mut wb).after_get, KifuAfterGet::Close);
    op(&mut wb, TencentKifuCommand::Open);
    assert_eq!(view(&mut wb).after_get, KifuAfterGet::Close);
    listed(&mut wb);
    download(&mut wb, "c0", Ok(json!({"result": 0, "chess": "(;GM[1]SZ[19];B[pd])"}).to_string()));
    let v = view(&mut wb);
    assert_eq!((v.opened, v.after_get), (1, KifuAfterGet::Close), "the opened record applies the frame's choice");
    // The next program start takes 野狐棋谱's choice again; the window's own choice persists.
    let mut restarted = workbench(dir.path());
    op(&mut restarted, TencentKifuCommand::Open);
    assert_eq!(view(&mut restarted).after_get, KifuAfterGet::Keep);
    op(&mut restarted, TencentKifuCommand::SetAfterGet { action: KifuAfterGet::Minimize });
    fox(&mut restarted, KifuAfterGet::Close);
    assert_eq!(view(&mut restarted).after_get, KifuAfterGet::Minimize);
    assert_eq!(workbench(dir.path()).config().tencent_kifu.after_get, Some(KifuAfterGet::Minimize));
}

#[test]
fn the_last_query_is_kept_and_personal_data_keeps_tencent_searches() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    listed(&mut wb);
    assert_eq!(workbench(dir.path()).config().tencent_kifu.last_name, "KeJie");
    // Java's 清除所有个人数据 lists 野狐 searches only.
    wb.clear_personal_data().unwrap();
    assert_eq!(view(&mut wb).recent.len(), 1);
    op(&mut wb, TencentKifuCommand::ClearRecent);
    assert!(view(&mut wb).recent.is_empty());
    assert!(workbench(dir.path()).config().tencent_kifu.recent.is_empty());
}

#[test]
fn recent_searches_keep_eight_newest_first() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    for q in ["a", "b", "c", "d", "e", "f", "g", "h", "i", "c"] {
        search(&mut wb, q);
        let games = json!({"result": 0, "chesslist": [game(0, "x", "y", 1)]}).to_string();
        serve(&mut wb, &mut Script::default().reply("username=", Ok(games)));
    }
    let queries: Vec<_> = view(&mut wb).recent.into_iter().map(|r| r.query).collect();
    assert_eq!(queries, ["c", "i", "h", "g", "f", "e", "d", "b"]);
}

#[test]
fn java_tencent_settings_are_imported_through_the_import_transaction() {
    let dir = tempfile::tempdir().unwrap();
    let java = dir.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let config = json!({"ui": {
        "fox-after-get": 2,
        "last-tencent-name": "KeJie", "tencent-after-get": 7,
        "tencent-recent-searches": [{"uid": "", "query": "KeJie"}, {"uid": "123", "query": ""}, {"uid": "", "query": ""}, 5],
    }});
    std::fs::write(java.join("config.txt"), config.to_string()).unwrap();
    let product = dir.path().join("product");
    let mut wb = workbench(&product);
    let preview = wb.preview_java_import(java_import::read(&JavaSource::new(&java, None)).unwrap());
    let row = |key: &str| preview.items.iter().find(|i| i.key == key).unwrap().clone();
    for key in ["ui.last-tencent-name", "ui.tencent-after-get", "ui.tencent-recent-searches"] {
        let item = row(key);
        assert_eq!(item.status, ImportStatus::Imported, "{key}");
        assert_eq!(item.target, Some(ImportTarget::TencentKifu), "{key}");
    }
    wb.apply_java_import(preview.id, &[], false, false, None).unwrap().unwrap();
    let reloaded = workbench(&product);
    let c = &reloaded.config().tencent_kifu;
    assert_eq!(c.last_name, "KeJie");
    // The combo clamps the saved index to its three entries.
    assert_eq!(c.after_get, Some(KifuAfterGet::Keep));
    assert_eq!(
        c.recent,
        vec![
            TencentRecentSearch { uid: String::new(), query: "KeJie".into() },
            TencentRecentSearch { uid: "123".into(), query: "123".into() },
        ]
    );

    // Without the window's own choice, the window takes 野狐棋谱's imported one.
    std::fs::write(java.join("config.txt"), json!({"ui": {"fox-after-get": 1}}).to_string()).unwrap();
    let product = dir.path().join("product2");
    let mut wb = workbench(&product);
    let preview = wb.preview_java_import(java_import::read(&JavaSource::new(&java, None)).unwrap());
    wb.apply_java_import(preview.id, &[], false, false, None).unwrap().unwrap();
    let mut reloaded = workbench(&product);
    assert_eq!(reloaded.config().tencent_kifu.after_get, None);
    op(&mut reloaded, TencentKifuCommand::Open);
    assert_eq!(view(&mut reloaded).after_get, KifuAfterGet::Close);
}

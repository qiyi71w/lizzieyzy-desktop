//! 腾讯棋谱 (`TencentKifuDownload`): the search session behind the window. One session runs per
//! search; a reply of an older session is dropped (a new search shuts the previous request down).
//! A downloaded record opens through the ordinary open path, with automatic komi loading off.

use super::{KifuSource, OpenOrigin, Workbench};
use crate::api::{
    ApiError, CommandResult, KifuNotice, KifuPage, TencentKifuBusy, TencentKifuCommand, TencentKifuRow,
    TencentKifuView,
};
use crate::record::RecordFormat;
use crate::settings::{KifuAfterGet, TencentRecentSearch, KIFU_RECENT_LIMIT};
use crate::tencent_kifu::{Game, Reply, Request, FETCH_NUM};

/// Games a page shows (`NUMBERS_PER_TAB`).
const PER_PAGE: usize = 25;
/// The `lastCode` of a search's first list.
const FIRST_LIST: &str = "0";

/// A request for the platform adapter to run ([`crate::tencent_kifu::fetch`]) and hand back with
/// [`Workbench::tencent_kifu_reply`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TencentKifuJob {
    /// The search it belongs to; the adapter may drop the queued jobs of an older one.
    pub session: u64,
    pub request: Request,
    /// `ui.no-capture` when the request started (a record is read with it).
    pub no_capture: bool,
    /// A record download's open request ([`Workbench::begin_open`], registered when 打开 is
    /// clicked): an open started meanwhile supersedes it, and its reply then changes nothing.
    pub open: Option<u64>,
}

#[derive(Default)]
pub(super) struct TencentKifuFlow {
    /// The window was opened once: its state starts from the saved search then.
    started: bool,
    /// 野狐棋谱's 打开棋谱后此窗口 when the window was first built: its default until chosen
    /// here (`optInt("tencent-after-get", foxAfterGet)` runs once, in the frame's constructor).
    fox_after_get: Option<KifuAfterGet>,
    /// The current search (`tencentReq`); 0 before the first one.
    session: u64,
    /// Bumped whenever a search starts and when the window first opens.
    search: u64,
    /// The searched text and, when it is all digits, the UID it names (`currentQuery`,
    /// `currentUid`).
    query: String,
    uid: String,
    /// 当前腾讯查询 as last drawn (`updateCurrentUserLabel`).
    label_query: String,
    /// Every game listed by this search (`tencentKifuInfos`), with whether the searched player
    /// won it.
    games: Vec<(Game, bool)>,
    /// Their rows exist (`rows != null`): set by the first list, cleared by a new search.
    listed: bool,
    /// The table's shown rows (the JTable keeps the previous search's rows until a new list
    /// replaces them).
    shown: Vec<TencentKifuRow>,
    page: u32,
    pages: u32,
    /// The page label as last drawn (`setLblTab`).
    label: (u32, u32, bool),
    complete: bool,
    second_empty: bool,
    request_empty: bool,
    advance_after_load: bool,
    cursor: Cursor,
    searching: bool,
    /// A record downloads or opens (`isKifuLoading`).
    loading: bool,
    busy: Option<TencentKifuBusy>,
    /// The open request of the downloaded record.
    open_ticket: Option<u64>,
    notice: Option<KifuNotice>,
    next_notice: u64,
    opened: u64,
    jobs: Vec<TencentKifuJob>,
}

/// `lastCode`: the last game a list was requested after. It advances when the request is sent (an
/// in-flight list cannot be requested twice) and steps back when that request fails, so the list
/// can be asked for again (Java kept the failed code, and no later games could be listed).
#[derive(Default)]
struct Cursor {
    current: String,
    pending_previous: Option<String>,
}

impl Cursor {
    fn should_request(&self, next: &str) -> bool {
        self.current != next
    }
    fn begin(&mut self, next: &str) {
        self.pending_previous = Some(std::mem::replace(&mut self.current, next.to_string()));
    }
    fn commit(&mut self) {
        self.pending_previous = None;
    }
    fn revert(&mut self) {
        if let Some(previous) = self.pending_previous.take() {
            self.current = previous;
        }
    }
}

fn refused(key: &str) -> CommandResult {
    CommandResult::Refused {
        message_key: key.into(),
    }
}

impl Workbench {
    pub fn take_tencent_kifu_jobs(&mut self) -> Vec<TencentKifuJob> {
        std::mem::take(&mut self.tencent_kifu.jobs)
    }

    pub fn tencent_kifu(&mut self, op: TencentKifuCommand) -> Result<CommandResult, ApiError> {
        match op {
            TencentKifuCommand::Open => {
                let f = &mut self.tencent_kifu;
                if !f.started {
                    // The window's first state: the saved query in the label, page 1/1.
                    f.started = true;
                    f.search += 1;
                    f.page = 1;
                    f.pages = 1;
                    f.label = (1, 1, true);
                    f.label_query = self.config.tencent_kifu.last_name.trim().to_string();
                    f.fox_after_get = Some(self.config.fox_kifu.after_get);
                }
                // `openTencentKifu`: browsing a downloaded record may want the analysis engine.
                self.preload_analysis_engine();
                Ok(CommandResult::Done)
            }
            TencentKifuCommand::Search { text } => Ok(self.tencent_kifu_search(&text)),
            TencentKifuCommand::Page { to } => Ok(self.tencent_kifu_page(to)),
            TencentKifuCommand::OpenGame { chessid } => {
                let f = &mut self.tencent_kifu;
                let chessid = chessid.trim();
                if f.session == 0 || chessid.is_empty() || f.loading {
                    return Ok(CommandResult::Unchanged);
                }
                f.loading = true;
                f.busy = Some(TencentKifuBusy::Downloading);
                let ticket = self.begin_open(OpenOrigin::Downloaded(KifuSource::Tencent));
                self.tencent_kifu.open_ticket = Some(ticket);
                let request = Request::Record {
                    chessid: chessid.into(),
                };
                self.tencent_kifu_job(request, Some(ticket));
                Ok(CommandResult::Done)
            }
            TencentKifuCommand::SetAfterGet { action } => {
                if self.config.tencent_kifu.after_get == Some(action) {
                    return Ok(CommandResult::Unchanged);
                }
                self.commit_config(|c| c.tencent_kifu.after_get = Some(action))?;
                Ok(CommandResult::Done)
            }
            TencentKifuCommand::ClearRecent => {
                if self.config.tencent_kifu.recent.is_empty() {
                    return Ok(CommandResult::Unchanged);
                }
                self.commit_config(|c| c.tencent_kifu.recent.clear())?;
                Ok(CommandResult::Done)
            }
        }
    }

    /// `triggerTencentSearch`: a new search replaces the current one.
    fn tencent_kifu_search(&mut self, text: &str) -> CommandResult {
        let text = text.trim();
        if text.is_empty() {
            return refused("TencentKifuDownload.noUser");
        }
        if self.tencent_kifu.searching {
            return refused("TencentKifuDownload.waitLastSearch");
        }
        if self.tencent_kifu.loading {
            return refused("KifuLoad.wait");
        }
        let f = &mut self.tencent_kifu;
        f.session += 1;
        f.search += 1;
        f.query = text.to_string();
        f.uid = if text.bytes().all(|b| b.is_ascii_digit()) {
            text.to_string()
        } else {
            String::new()
        };
        f.label_query = text.to_string();
        f.searching = true;
        f.complete = false;
        f.second_empty = false;
        f.request_empty = false;
        f.advance_after_load = false;
        f.games.clear();
        f.listed = false;
        f.page = 1;
        f.pages = 1;
        f.cursor = Cursor::default();
        f.busy = Some(TencentKifuBusy::Searching { text: text.to_string() });
        // Remembered even if the settings cannot be written (`saveConfigQuietly`).
        self.config.tencent_kifu.last_name = text.to_string();
        let _ = self.save_config();
        let request = Request::List {
            query: text.to_string(),
            last_code: FIRST_LIST.into(),
        };
        self.tencent_kifu_job(request, None);
        CommandResult::Done
    }

    fn tencent_kifu_job(&mut self, request: Request, open: Option<u64>) {
        let job = TencentKifuJob {
            session: self.tencent_kifu.session,
            request,
            no_capture: self.config.edit.no_capture,
            open,
        };
        self.tencent_kifu.jobs.push(job);
    }

    /// The page buttons (|< < > >|).
    fn tencent_kifu_page(&mut self, to: KifuPage) -> CommandResult {
        let f = &mut self.tencent_kifu;
        if !f.listed {
            return CommandResult::Unchanged;
        }
        match to {
            KifuPage::First => {
                f.page = 1;
                f.second_empty = false;
            }
            KifuPage::Previous => {
                if f.page == 1 {
                    return CommandResult::Unchanged;
                }
                f.page -= 1;
                f.second_empty = false;
            }
            KifuPage::Next | KifuPage::Last => {
                if f.page == f.pages {
                    return self.tencent_kifu_more();
                }
                f.page = if to == KifuPage::Next { f.page + 1 } else { f.pages };
            }
        }
        f.show_page();
        if matches!(to, KifuPage::Next | KifuPage::Last) {
            let more = self.tencent_kifu_more();
            if matches!(more, CommandResult::Refused { .. }) {
                return more;
            }
        }
        CommandResult::Done
    }

    /// `maybeGetNextPage`: on the last page (or the one before it once there are four) the games
    /// after the last listed one are fetched; with none left, the second try says so.
    fn tencent_kifu_more(&mut self) -> CommandResult {
        let f = &mut self.tencent_kifu;
        if f.session == 0 || f.loading {
            return CommandResult::Unchanged;
        }
        let Some(last) = f.games.last().map(|(g, _)| g.chessid.clone()) else {
            return CommandResult::Unchanged;
        };
        if !(f.page == f.pages || f.pages >= 4 && f.page == f.pages - 1) {
            return CommandResult::Done;
        }
        if f.cursor.should_request(&last) {
            f.cursor.begin(&last);
            f.advance_after_load = f.page == f.pages;
            f.busy = Some(TencentKifuBusy::LoadingMore);
            let request = Request::List {
                query: f.query.clone(),
                last_code: last,
            };
            self.tencent_kifu_job(request, None);
            return CommandResult::Done;
        }
        if f.page == f.pages {
            let shown = f.second_empty;
            if f.request_empty {
                f.second_empty = true;
            }
            if shown {
                return refused("TencentKifuDownload.noMoreKifu");
            }
        }
        CommandResult::Done
    }

    /// A finished request. Returns whether the window's state changed (a reply of a replaced
    /// search, or of a download another open superseded, changes nothing).
    pub fn tencent_kifu_reply(&mut self, job: &TencentKifuJob, reply: Reply) -> bool {
        if job.session != self.tencent_kifu.session || job.open.is_some_and(|ticket| !self.is_current_open(ticket)) {
            return false;
        }
        let more = matches!(&job.request, Request::List { last_code, .. } if last_code != FIRST_LIST);
        match reply {
            Reply::Failed { message } => {
                // The window reports a failed download itself; its open request just ends.
                if let Some(ticket) = job.open {
                    self.abandon_open(ticket);
                }
                // `handleRequestFailure`, limited to the failed request: a failed list neither
                // ends a record download nor hides its progress.
                let f = &mut self.tencent_kifu;
                f.searching = false;
                if more {
                    f.cursor.revert();
                    f.advance_after_load = false;
                }
                let own = match f.busy {
                    Some(TencentKifuBusy::Searching { .. }) => matches!(job.request, Request::List { .. }) && !more,
                    Some(TencentKifuBusy::LoadingMore) => more,
                    Some(TencentKifuBusy::Downloading | TencentKifuBusy::Opening) => job.open.is_some(),
                    None => false,
                };
                if own {
                    f.busy = None;
                }
                f.notify("TencentKifuDownload.getKifuFailed", message);
            }
            Reply::Games { games, fetched } => self.tencent_kifu_games(games, fetched),
            Reply::Record(read) => {
                let Some(ticket) = job.open else {
                    return false;
                };
                self.tencent_kifu.busy = Some(TencentKifuBusy::Opening);
                self.complete_open(ticket, read.map(|r| (RecordFormat::Sgf, r)));
            }
        }
        true
    }

    /// `handleChessList`.
    fn tencent_kifu_games(&mut self, games: Vec<Game>, fetched: usize) {
        let f = &mut self.tencent_kifu;
        f.searching = false;
        f.cursor.commit();
        // The list's progress ends; a record downloading meanwhile keeps its own.
        if matches!(f.busy, Some(TencentKifuBusy::Searching { .. } | TencentKifuBusy::LoadingMore)) {
            f.busy = None;
        }
        let old = f.games.len();
        let previous_page = f.page;
        let advance = std::mem::take(&mut f.advance_after_load);
        if fetched == 0 {
            if old > 0 {
                f.complete = true;
                f.second_empty = true;
                f.request_empty = true;
            } else {
                f.notify("TencentKifuDownload.noKifu", String::new());
            }
            return;
        }
        f.request_empty = false;
        f.complete = fetched < FETCH_NUM;
        let (query, uid) = (f.query.clone(), f.uid.clone());
        f.games.extend(games.into_iter().map(|g| {
            let won = won(&g, &query, &uid);
            (g, won)
        }));
        f.label_query = query.clone();
        let first = old == 0 || !f.listed;
        f.listed = true;
        f.pages = f.games.len().div_ceil(PER_PAGE).max(1) as u32;
        f.page = if first {
            1
        } else if advance && previous_page < f.pages {
            previous_page + 1
        } else {
            previous_page.min(f.pages)
        };
        f.show_page();
        self.tencent_kifu_remember(&query, &uid);
    }

    /// `addRecentSearch`: the query moves to the front, replacing entries with its UID or text.
    fn tencent_kifu_remember(&mut self, query: &str, uid: &str) {
        let recent = &mut self.config.tencent_kifu.recent;
        recent.retain(|r| !(!uid.is_empty() && r.uid == uid || r.query == query));
        recent.insert(
            0,
            TencentRecentSearch {
                uid: uid.to_string(),
                query: query.to_string(),
            },
        );
        recent.truncate(KIFU_RECENT_LIMIT);
        let _ = self.save_config();
    }

    /// The open request a downloaded record started ended: replaced by another open, refused, or
    /// done (`completeTencentKifuLoadRequest`).
    pub(super) fn tencent_kifu_open_ended(&mut self, ticket: u64, adopted: bool) {
        let f = &mut self.tencent_kifu;
        if f.open_ticket != Some(ticket) {
            return;
        }
        f.open_ticket = None;
        f.loading = false;
        f.busy = None;
        if adopted {
            f.opened += 1;
        }
    }

    pub(super) fn tencent_kifu_view(&self) -> TencentKifuView {
        let f = &self.tencent_kifu;
        let saved = &self.config.tencent_kifu;
        TencentKifuView {
            search: f.search,
            last_name: saved.last_name.clone(),
            // Before the window is built, the 野狐棋谱 choice it would take.
            after_get: saved
                .after_get
                .or(f.fox_after_get)
                .unwrap_or(self.config.fox_kifu.after_get),
            recent: saved.recent.clone(),
            query: f.label_query.clone(),
            rows: f.shown.clone(),
            listed: f.listed,
            page: f.label.0,
            pages: f.label.1,
            complete: f.label.2,
            busy: f.busy.clone(),
            loading: f.loading,
            notice: f.notice.clone(),
            opened: f.opened,
        }
    }
}

/// `isCurrentUserWin`: the winner is the searched UID, or (searching a nickname) has its name,
/// letter case aside.
fn won(g: &Game, query: &str, uid: &str) -> bool {
    let (winner_uid, winner_name) = match g.winner {
        1 => (&g.black_uid, &g.black_name),
        2 => (&g.white_uid, &g.white_name),
        _ => return false,
    };
    if !uid.is_empty() {
        return uid == winner_uid;
    }
    !query.is_empty() && winner_name.as_ref().is_some_and(|name| name.to_lowercase() == query.to_lowercase())
}

impl TencentKifuFlow {
    /// `reloadCurrentPage` + `setLblTab`.
    fn show_page(&mut self) {
        let start = (self.page as usize - 1) * PER_PAGE;
        self.shown = self
            .games
            .iter()
            .enumerate()
            .skip(start)
            .take(PER_PAGE)
            .map(|(i, (g, won))| row(i, g, *won))
            .collect();
        self.label = (self.page, self.pages, self.complete);
    }

    fn notify(&mut self, key: &str, detail: String) {
        self.next_notice += 1;
        self.notice = Some(KifuNotice {
            id: self.next_notice,
            message_key: key.into(),
            detail,
        });
    }
}

fn row(index: usize, g: &Game, won: bool) -> TencentKifuRow {
    TencentKifuRow {
        index: index as u32 + 1,
        time: g.start_time.clone(),
        start_seconds: g.start_seconds,
        black_name: g.black_name.clone(),
        black_uid: g.black_uid.clone(),
        black_rank: g.black_rank,
        white_name: g.white_name.clone(),
        white_uid: g.white_uid.clone(),
        white_rank: g.white_rank,
        winner: g.winner,
        point: g.point,
        rule: g.rule,
        moves: g.moves,
        won,
        chessid: g.chessid.clone(),
    }
}

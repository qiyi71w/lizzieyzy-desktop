//! 野狐棋谱 (`FoxKifuDownload`): the search session behind the window. One session runs per
//! search; a reply of an older session is dropped (`receiveResult`: only the current request may
//! touch the dialog). A downloaded record opens through the ordinary open path.

use super::{KifuSource, OpenOrigin, Workbench};
use crate::api::{
    ApiError, CommandResult, FoxKifuBusy, FoxKifuCommand, KifuNotice, KifuPage, FoxKifuRow, FoxKifuView,
};
use crate::fox_kifu::{Game, Reply, Request};
use crate::record::RecordFormat;
use crate::settings::{FoxRecentSearch, KIFU_RECENT_LIMIT};

/// Games a page shows (`numbersPerTab`).
const PER_PAGE: usize = 25;
/// A full page from Fox (`isComplete = length < 100`).
const FOX_PAGE: usize = 100;

/// A request for the platform adapter to run ([`crate::fox_kifu::fetch`]) and hand back with
/// [`Workbench::fox_kifu_reply`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoxKifuJob {
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
pub(super) struct FoxKifuFlow {
    /// The window was opened once: its state starts from the saved search then.
    started: bool,
    /// The current search (`foxReq`); 0 before the first one.
    session: u64,
    /// Bumped whenever a search starts and when the window first opens.
    search: u64,
    /// The searched account (`myUid`, `currentFoxNickname`).
    uid: String,
    nickname: String,
    /// 当前野狐账号 as last drawn (`updateCurrentUserLabel`): nickname, UID.
    label_user: (String, String),
    /// Every game listed by this search (`foxKifuInfos`).
    games: Vec<Game>,
    /// Their rows exist (`rows != null`): set by the first page, cleared by a new search.
    listed: bool,
    /// The table's shown rows (the JTable model keeps the previous search's rows until a new
    /// list replaces them).
    shown: Vec<FoxKifuRow>,
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
    busy: Option<FoxKifuBusy>,
    /// The open request of the downloaded record.
    open_ticket: Option<u64>,
    notice: Option<KifuNotice>,
    next_notice: u64,
    opened: u64,
    jobs: Vec<FoxKifuJob>,
}

/// `FoxPaginationCursor`: the last game a page was requested after. It advances when the request
/// is sent (an in-flight page cannot be requested twice) and steps back when that request fails.
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
    pub fn take_fox_kifu_jobs(&mut self) -> Vec<FoxKifuJob> {
        std::mem::take(&mut self.fox_kifu.jobs)
    }

    pub fn fox_kifu(&mut self, op: FoxKifuCommand) -> Result<CommandResult, ApiError> {
        match op {
            FoxKifuCommand::Open => {
                let f = &mut self.fox_kifu;
                if !f.started {
                    // The dialog's first state: the saved account in the label, page 1/1.
                    f.started = true;
                    f.search += 1;
                    f.page = 1;
                    f.pages = 1;
                    f.label = (1, 1, true);
                    let saved = self.config.fox_kifu.last_name.trim().to_string();
                    f.label_user = if self.config.fox_kifu.last_query_by_uid {
                        (String::new(), saved)
                    } else {
                        (saved, String::new())
                    };
                }
                // `openFoxReq`: browsing a downloaded record may want the analysis engine.
                self.preload_analysis_engine();
                Ok(CommandResult::Done)
            }
            FoxKifuCommand::Search {
                text,
                by_uid,
                nickname_hint,
            } => Ok(self.fox_kifu_search(&text, by_uid, &nickname_hint)),
            FoxKifuCommand::Page { to } => Ok(self.fox_kifu_page(to)),
            FoxKifuCommand::OpenGame { chessid } => {
                let f = &mut self.fox_kifu;
                let chessid = chessid.trim();
                if f.session == 0 || chessid.is_empty() || f.loading {
                    return Ok(CommandResult::Unchanged);
                }
                f.loading = true;
                f.busy = Some(FoxKifuBusy::Downloading);
                let ticket = self.begin_open(OpenOrigin::Downloaded(KifuSource::Fox));
                self.fox_kifu.open_ticket = Some(ticket);
                let request = Request::Record {
                    chessid: chessid.into(),
                };
                self.fox_kifu_job(request, Some(ticket));
                Ok(CommandResult::Done)
            }
            FoxKifuCommand::SetAfterGet { action } => {
                if self.config.fox_kifu.after_get == action {
                    return Ok(CommandResult::Unchanged);
                }
                self.commit_config(|c| c.fox_kifu.after_get = action)?;
                Ok(CommandResult::Done)
            }
            FoxKifuCommand::ClearRecent => {
                if self.config.fox_kifu.recent.is_empty() {
                    return Ok(CommandResult::Unchanged);
                }
                self.commit_config(|c| c.fox_kifu.recent.clear())?;
                Ok(CommandResult::Done)
            }
        }
    }

    /// `triggerFoxSearch`: a new search replaces the current one.
    fn fox_kifu_search(&mut self, text: &str, by_uid: bool, nickname_hint: &str) -> CommandResult {
        let text = text.trim();
        if text.is_empty() {
            return refused(if by_uid {
                "FoxKifuDownload.noUid"
            } else {
                "FoxKifuDownload.noUser"
            });
        }
        if by_uid && !text.bytes().all(|b| b.is_ascii_digit()) {
            return refused("FoxKifuDownload.invalidUidInput");
        }
        if self.fox_kifu.searching {
            return refused("FoxKifuDownload.waitLastSearch");
        }
        if self.fox_kifu.loading {
            return refused("KifuLoad.wait");
        }
        let hint = nickname_hint.trim();
        let f = &mut self.fox_kifu;
        f.session += 1;
        f.search += 1;
        f.uid = String::new();
        f.nickname = if by_uid { hint.to_string() } else { text.to_string() };
        f.label_user = if by_uid {
            (hint.to_string(), text.to_string())
        } else {
            (text.to_string(), String::new())
        };
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
        f.busy = Some(FoxKifuBusy::Searching { text: text.to_string() });
        // Remembered even if the settings cannot be written (`saveConfigQuietly`).
        self.config.fox_kifu.last_name = text.to_string();
        self.config.fox_kifu.last_query_by_uid = by_uid;
        let _ = self.save_config();
        let request = if by_uid {
            Request::Account {
                uid: text.to_string(),
                nickname: hint.to_string(),
            }
        } else {
            Request::Nickname { name: text.to_string() }
        };
        self.fox_kifu_job(request, None);
        CommandResult::Done
    }

    fn fox_kifu_job(&mut self, request: Request, open: Option<u64>) {
        let job = FoxKifuJob {
            session: self.fox_kifu.session,
            request,
            no_capture: self.config.edit.no_capture,
            open,
        };
        self.fox_kifu.jobs.push(job);
    }

    /// The page buttons (`FoxKifuDownload` |< < > >|).
    fn fox_kifu_page(&mut self, to: KifuPage) -> CommandResult {
        let f = &mut self.fox_kifu;
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
                    return self.fox_kifu_more();
                }
                f.page = if to == KifuPage::Next { f.page + 1 } else { f.pages };
            }
        }
        f.show_page();
        if matches!(to, KifuPage::Next | KifuPage::Last) {
            let more = self.fox_kifu_more();
            if matches!(more, CommandResult::Refused { .. }) {
                return more;
            }
        }
        CommandResult::Done
    }

    /// `maybeGetNextPage`: on the last page (or the one before it once there are four) the games
    /// after the last listed one are fetched; with none left, the second try says so.
    fn fox_kifu_more(&mut self) -> CommandResult {
        let f = &mut self.fox_kifu;
        if f.session == 0 || f.loading {
            return CommandResult::Unchanged;
        }
        let Some(last) = f.games.last().map(|g| g.chessid.clone()) else {
            return CommandResult::Unchanged;
        };
        if !(f.page == f.pages || f.pages >= 4 && f.page == f.pages - 1) {
            return CommandResult::Done;
        }
        if f.cursor.should_request(&last) {
            f.cursor.begin(&last);
            f.advance_after_load = f.page == f.pages;
            let request = Request::Page {
                uid: f.uid.clone(),
                last_code: last,
            };
            self.fox_kifu_job(request, None);
            return CommandResult::Done;
        }
        if f.page == f.pages {
            let shown = f.second_empty;
            if f.request_empty {
                f.second_empty = true;
            }
            if shown {
                return refused("FoxKifuDownload.noMoreKifu");
            }
        }
        CommandResult::Done
    }

    /// A finished request. Returns whether the window's state changed (a reply of a replaced
    /// search, or of a download another open superseded, changes nothing).
    pub fn fox_kifu_reply(&mut self, job: &FoxKifuJob, reply: Reply) -> bool {
        if job.session != self.fox_kifu.session || job.open.is_some_and(|ticket| !self.is_current_open(ticket)) {
            return false;
        }
        let page = matches!(job.request, Request::Page { .. });
        match reply {
            Reply::Failed {
                message,
                user_not_found,
            } => {
                // The window reports a failed download itself; its open request just ends.
                if let Some(ticket) = job.open {
                    self.abandon_open(ticket);
                }
                // `failCurrentFoxRequest`, limited to the failed request: a failed page neither
                // ends a record download nor hides its progress (Java let any failure do both).
                let f = &mut self.fox_kifu;
                f.searching = false;
                if page {
                    f.cursor.revert();
                    f.advance_after_load = false;
                } else {
                    f.busy = None;
                }
                if user_not_found {
                    f.notify("FoxKifuDownload.invalidUid", String::new());
                } else {
                    f.notify("FoxKifuDownload.getKifuFailed", message);
                }
            }
            Reply::Games { uid, nickname, games } => self.fox_kifu_games(uid, nickname, games),
            Reply::Record(read) => {
                let Some(ticket) = job.open else {
                    return false;
                };
                self.fox_kifu.busy = Some(FoxKifuBusy::Opening);
                self.complete_open(ticket, read.map(|r| (RecordFormat::Sgf, r)));
            }
        }
        true
    }

    /// `handleResult` + `handleChessList`.
    fn fox_kifu_games(&mut self, uid: String, nickname: String, games: Vec<Game>) {
        let f = &mut self.fox_kifu;
        if !uid.is_empty() {
            f.uid = uid;
        }
        if !nickname.is_empty() {
            f.nickname = nickname;
        }
        f.searching = false;
        f.cursor.commit();
        // The search's progress ends; a record downloading meanwhile keeps its own.
        if matches!(f.busy, Some(FoxKifuBusy::Searching { .. })) {
            f.busy = None;
        }
        let old = f.games.len();
        let previous_page = f.page;
        let advance = std::mem::take(&mut f.advance_after_load);
        if games.is_empty() {
            if old > 0 {
                f.complete = true;
                f.second_empty = true;
                f.request_empty = true;
            } else {
                f.notify("FoxKifuDownload.noKifu", String::new());
            }
            return;
        }
        f.complete = games.len() < FOX_PAGE;
        let uid = f.uid.clone();
        let detected = games.iter().find_map(|g| {
            if g.black_uid == uid {
                Some(g.black_name.clone())
            } else if g.white_uid == uid {
                Some(g.white_name.clone())
            } else {
                None
            }
        });
        f.games.extend(games);
        if let Some(Some(name)) = detected {
            f.nickname = name;
        }
        f.label_user = (f.nickname.clone(), f.uid.clone());
        f.listed = true;
        f.pages = f.games.len().div_ceil(PER_PAGE).max(1) as u32;
        f.page = if old == 0 {
            1
        } else if advance && previous_page < f.pages {
            previous_page + 1
        } else {
            previous_page.min(f.pages)
        };
        f.show_page();
        let (nickname, uid) = f.label_user.clone();
        if !nickname.is_empty() || !uid.is_empty() {
            self.fox_kifu_remember(&nickname, &uid);
        }
    }

    /// `addRecentSearch`: the account moves to the front, replacing entries with its UID or
    /// nickname.
    fn fox_kifu_remember(&mut self, nickname: &str, uid: &str) {
        let recent = &mut self.config.fox_kifu.recent;
        recent.retain(|r| !(!uid.is_empty() && r.uid == uid || !nickname.is_empty() && r.nickname == nickname));
        recent.insert(
            0,
            FoxRecentSearch {
                uid: uid.to_string(),
                nickname: nickname.to_string(),
            },
        );
        recent.truncate(KIFU_RECENT_LIMIT);
        let _ = self.save_config();
    }

    /// The open request a downloaded record started ended: replaced by another open, refused, or
    /// done (`completeFoxKifuLoadRequest`).
    pub(super) fn fox_kifu_open_ended(&mut self, ticket: u64, adopted: bool) {
        let f = &mut self.fox_kifu;
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

    pub(super) fn fox_kifu_view(&self) -> FoxKifuView {
        let f = &self.fox_kifu;
        let saved = &self.config.fox_kifu;
        FoxKifuView {
            search: f.search,
            last_name: saved.last_name.clone(),
            last_query_by_uid: saved.last_query_by_uid,
            after_get: saved.after_get,
            recent: saved.recent.clone(),
            nickname: f.label_user.0.clone(),
            uid: f.label_user.1.clone(),
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

impl FoxKifuFlow {
    /// `reloadCurrentPage` + `setLblTab`.
    fn show_page(&mut self) {
        let start = (self.page as usize - 1) * PER_PAGE;
        let uid = &self.uid;
        self.shown = self
            .games
            .iter()
            .enumerate()
            .skip(start)
            .take(PER_PAGE)
            .map(|(i, g)| row(i, g, uid))
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

fn row(index: usize, g: &Game, uid: &str) -> FoxKifuRow {
    FoxKifuRow {
        index: index as u32 + 1,
        time: g.start_time.clone(),
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
        won: match g.winner {
            1 => g.black_uid == uid,
            2 => g.white_uid == uid,
            _ => false,
        },
        chessid: g.chessid.clone(),
    }
}

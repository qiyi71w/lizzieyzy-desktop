//! OGS 棋谱 (Java `OgsKifuBrowser`, `OgsKifuImport`): the window's search session and the one-shot
//! import of an OGS link from 打开在线链接. Each request has its own id; the window has at most one
//! request at a time, and a reply whose id is not the current one changes nothing (Java's
//! `generation`). A request ends unanswered when it is no longer wanted: closing the window ends
//! the window's (and withdraws the open of a record it downloaded, even one waiting for a
//! borrowed engine), stopping 打开在线链接 ends the link's, and another open replacing a record's
//! open ends its download. A downloaded record opens through the ordinary open path with the
//! record's komi read (`loadDownloadedSgfString(sgf, 0, true, …)`), so a failed download or read
//! keeps the current game.

use super::{KifuSource, OpenOrigin, Workbench};
use crate::api::{
    ApiError, CommandResult, KifuPage, OgsKifuCommand, OgsKifuLabel, OgsKifuRow, OgsKifuStatus, OgsKifuView,
    OpenOutcome,
};
use crate::ogs_kifu::{Game, GamePage, OgsFailure, Player, Reply, Request};
use crate::record::RecordFormat;
use crate::settings::KIFU_RECENT_LIMIT;

/// Java `OgsKifuImport.failed`: an OGS link whose game could not be downloaded.
pub const OGS_LINK_FAILED: &str = "OgsKifuImport.failed";
/// The product message for an OGS link refused with HTTP 429.
pub const OGS_LINK_RATE_LIMITED: &str = "ogsRecordRateLimited";

/// Work for the platform adapter: run a request ([`crate::ogs_kifu::fetch`]) and hand its reply to
/// [`Workbench::ogs_kifu_reply`], or stop one that is no longer wanted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OgsKifuTask {
    Fetch(OgsKifuJob),
    /// The request `id` was cancelled: anything it still sends is dropped.
    Cancel(u64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OgsKifuJob {
    pub id: u64,
    pub request: Request,
    /// `ui.no-capture` when the request started (a record is read with it).
    pub no_capture: bool,
}

/// What the window's current request is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Purpose {
    Search,
    Page,
    /// A game download, with the open request it started.
    Record(u64),
}

/// The 打开在线链接 import of an OGS link: its open request, and its download while that runs.
struct LinkImport {
    download: Option<u64>,
    ticket: u64,
}

#[derive(Default)]
pub(super) struct OgsKifuFlow {
    next_id: u64,
    /// The window's request in flight.
    request: Option<(u64, Purpose)>,
    /// The 打开在线链接 import, until its open request ends.
    link: Option<LinkImport>,
    /// The open request of a record downloaded by the window, until it ends.
    open_ticket: Option<u64>,
    player: Option<Player>,
    label: Option<OgsKifuLabel>,
    games: Vec<Game>,
    page: u32,
    pages: u32,
    shown: u64,
    status: Option<OgsKifuStatus>,
    opened: u64,
    tasks: Vec<OgsKifuTask>,
}

impl Workbench {
    pub fn take_ogs_kifu_tasks(&mut self) -> Vec<OgsKifuTask> {
        std::mem::take(&mut self.ogs_kifu.tasks)
    }

    pub fn ogs_kifu(&mut self, op: OgsKifuCommand) -> Result<CommandResult, ApiError> {
        match op {
            OgsKifuCommand::Open => {
                // `openKifu`: browsing a downloaded record may want the analysis engine.
                self.preload_analysis_engine();
                Ok(CommandResult::Done)
            }
            OgsKifuCommand::Search { text } => Ok(self.ogs_kifu_search(&text)),
            OgsKifuCommand::Page { to } => Ok(self.ogs_kifu_page(to)),
            OgsKifuCommand::OpenGame { id } => Ok(self.ogs_kifu_open_game(id)),
            OgsKifuCommand::SetAfterGet { action } => {
                if self.config.ogs_kifu.after_get == Some(action) {
                    return Ok(CommandResult::Unchanged);
                }
                self.commit_config(|c| c.ogs_kifu.after_get = Some(action))?;
                Ok(CommandResult::Done)
            }
            OgsKifuCommand::ClearRecent => {
                if self.config.ogs_kifu.recent.is_empty() {
                    return Ok(CommandResult::Unchanged);
                }
                self.commit_config(|c| c.ogs_kifu.recent.clear())?;
                Ok(CommandResult::Done)
            }
            OgsKifuCommand::Close => Ok(self.ogs_kifu_close()),
        }
    }

    fn ogs_kifu_busy(&self) -> bool {
        self.ogs_kifu.request.is_some() || self.ogs_kifu.open_ticket.is_some()
    }

    /// Whether request `id` is still waited for: the platform adapter sends nothing for one that
    /// is not (it may have been cancelled before the adapter registered it).
    pub fn ogs_kifu_wanted(&self, id: u64) -> bool {
        let f = &self.ogs_kifu;
        f.request.is_some_and(|(r, _)| r == id) || f.link.as_ref().is_some_and(|l| l.download == Some(id))
    }

    fn ogs_kifu_fetch(&mut self, request: Request) -> u64 {
        let f = &mut self.ogs_kifu;
        f.next_id += 1;
        let id = f.next_id;
        f.tasks.push(OgsKifuTask::Fetch(OgsKifuJob {
            id,
            request,
            no_capture: self.config.edit.no_capture,
        }));
        id
    }

    /// `OgsKifuBrowser.search`: the previous player and rows go at once, so a failed search never
    /// leaves another player's games to open.
    fn ogs_kifu_search(&mut self, text: &str) -> CommandResult {
        if self.ogs_kifu_busy() {
            return CommandResult::Unchanged;
        }
        let text = text.trim_matches(|c: char| c <= ' ');
        if text.is_empty() {
            self.ogs_kifu.status = Some(OgsKifuStatus::NeedPlayer);
            return CommandResult::Done;
        }
        let id = self.ogs_kifu_fetch(Request::Search {
            query: text.to_string(),
        });
        let f = &mut self.ogs_kifu;
        f.player = None;
        f.games.clear();
        f.page = 1;
        f.pages = 1;
        f.label = Some(OgsKifuLabel::Searched { text: text.to_string() });
        f.status = Some(OgsKifuStatus::Downloading);
        f.request = Some((id, Purpose::Search));
        CommandResult::Done
    }

    /// `fetchPage`.
    fn ogs_kifu_page(&mut self, to: KifuPage) -> CommandResult {
        if self.ogs_kifu_busy() {
            return CommandResult::Unchanged;
        }
        let f = &self.ogs_kifu;
        let Some(player) = f.player.clone() else {
            return CommandResult::Unchanged;
        };
        let target = match to {
            KifuPage::First => 1,
            KifuPage::Previous => f.page.saturating_sub(1),
            KifuPage::Next => f.page + 1,
            KifuPage::Last => f.pages,
        };
        if target < 1 || target > f.pages || target == f.page {
            return CommandResult::Unchanged;
        }
        let id = self.ogs_kifu_fetch(Request::Page { player, page: target });
        self.ogs_kifu.status = Some(OgsKifuStatus::Downloading);
        self.ogs_kifu.request = Some((id, Purpose::Page));
        CommandResult::Done
    }

    /// `openSelected`: one download at a time; the record then opens like a file.
    fn ogs_kifu_open_game(&mut self, game: u64) -> CommandResult {
        if self.ogs_kifu_busy() || !self.ogs_kifu.games.iter().any(|g| g.id == game) {
            return CommandResult::Unchanged;
        }
        let ticket = self.begin_open(OpenOrigin::Downloaded(KifuSource::Ogs));
        let id = self.ogs_kifu_fetch(Request::Record { game: game.to_string() });
        let f = &mut self.ogs_kifu;
        f.open_ticket = Some(ticket);
        f.status = Some(OgsKifuStatus::Downloading);
        f.request = Some((id, Purpose::Record(ticket)));
        CommandResult::Done
    }

    /// The window closed (`OgsKifuDownload.dispose` → `cancelPending`): its request is cancelled
    /// and the open request of a record it downloaded withdrawn, whether the record is still
    /// downloading or waits for a borrowed engine to come back; the next opening starts from an
    /// empty window.
    fn ogs_kifu_close(&mut self) -> CommandResult {
        if let Some((id, _)) = self.ogs_kifu.request.take() {
            self.ogs_kifu.tasks.push(OgsKifuTask::Cancel(id));
        }
        if let Some(ticket) = self.ogs_kifu.open_ticket.take() {
            // Ended here, so the window does not hear of it as a failed open.
            self.withdraw_open(ticket);
            // A preload waiting for this read decides now.
            self.warmup_review();
        }
        let f = &mut self.ogs_kifu;
        let changed = f.player.is_some() || !f.games.is_empty() || f.label.is_some() || f.status.is_some();
        f.player = None;
        f.games.clear();
        f.page = 1;
        f.pages = 1;
        f.label = None;
        f.status = None;
        if changed {
            CommandResult::Done
        } else {
            CommandResult::Unchanged
        }
    }

    /// 打开在线链接 with an OGS game or SGF address (`OgsKifuImport.tryOpen`): the record is
    /// downloaded once and opened; a failure says so and keeps the current game. The dialog
    /// stops any earlier import first ([`Workbench::ogs_kifu_link_stop`]).
    pub(super) fn ogs_kifu_link(&mut self, game: String) {
        let ticket = self.begin_open(OpenOrigin::Downloaded(KifuSource::Ogs));
        let id = self.ogs_kifu_fetch(Request::Record { game });
        self.ogs_kifu.link = Some(LinkImport {
            download: Some(id),
            ticket,
        });
    }

    /// 打开在线链接 stopped (reopened, or another link opened): the OGS import ends without a
    /// record or a message, its download cancelled and its open request withdrawn.
    pub(super) fn ogs_kifu_link_stop(&mut self) {
        let Some(link) = self.ogs_kifu.link.take() else {
            return;
        };
        if let Some(id) = link.download {
            self.ogs_kifu.tasks.push(OgsKifuTask::Cancel(id));
        }
        self.withdraw_open(link.ticket);
        self.warmup_review();
    }

    /// A finished request. Returns whether anything changed: a reply of a cancelled or replaced
    /// request changes nothing.
    pub fn ogs_kifu_reply(&mut self, job: &OgsKifuJob, reply: Reply) -> bool {
        if let Some(link) = self.ogs_kifu.link.as_mut() {
            if link.download == Some(job.id) {
                link.download = None;
                let ticket = link.ticket;
                self.ogs_kifu_link_reply(ticket, reply);
                return true;
            }
        }
        let Some((id, purpose)) = self.ogs_kifu.request else {
            return false;
        };
        if id != job.id {
            return false;
        }
        self.ogs_kifu.request = None;
        match (purpose, reply) {
            (Purpose::Record(ticket), Reply::Record(read)) => {
                self.ogs_kifu.status = Some(OgsKifuStatus::Opening);
                if !self.complete_open(ticket, read.map(|r| (RecordFormat::Sgf, r))) {
                    // Another open replaced this one meanwhile.
                    self.ogs_kifu.status = None;
                }
            }
            (Purpose::Record(ticket), reply) => {
                let failure = match reply {
                    Reply::Failed(failure) => failure,
                    _ => OgsFailure::Unavailable,
                };
                self.ogs_kifu.open_ticket = None;
                self.ogs_kifu.status = Some(OgsKifuStatus::Failed { failure, record: true });
                self.abandon_open(ticket);
            }
            (_, Reply::Found { player, page }) => {
                self.ogs_kifu.label = Some(OgsKifuLabel::Player {
                    name: player.name.clone(),
                    id: player.id,
                });
                self.ogs_kifu.player = Some(player);
                if let Request::Search { query } = &job.request {
                    self.ogs_kifu_remember(query);
                }
                self.ogs_kifu_show(page);
            }
            (_, Reply::Page(page)) => self.ogs_kifu_show(page),
            (_, Reply::NoPlayer) => self.ogs_kifu.status = Some(OgsKifuStatus::NoPlayer),
            (_, Reply::Failed(failure)) => {
                self.ogs_kifu.status = Some(OgsKifuStatus::Failed { failure, record: false });
            }
            (_, Reply::Record(_)) => {
                self.ogs_kifu.status = Some(OgsKifuStatus::Failed {
                    failure: OgsFailure::Unavailable,
                    record: false,
                })
            }
        }
        true
    }

    /// The link's download answered; its import is current (another open would have ended it).
    fn ogs_kifu_link_reply(&mut self, ticket: u64, reply: Reply) {
        match reply {
            Reply::Record(read) => {
                self.complete_open(ticket, read.map(|r| (RecordFormat::Sgf, r)));
            }
            other => {
                let message_key = match other {
                    Reply::Failed(OgsFailure::RateLimited) => OGS_LINK_RATE_LIMITED,
                    _ => OGS_LINK_FAILED,
                };
                self.abandon_open(ticket);
                self.outcome = Some(OpenOutcome::Failed {
                    ticket,
                    message_key: message_key.into(),
                    detail: String::new(),
                });
            }
        }
    }

    /// `showPage`.
    fn ogs_kifu_show(&mut self, page: GamePage) {
        let f = &mut self.ogs_kifu;
        f.status = if page.games.is_empty() {
            Some(OgsKifuStatus::Empty)
        } else {
            None
        };
        f.games = page.games;
        f.page = page.page;
        f.pages = page.pages;
        f.shown += 1;
    }

    /// The recent searches: the text that found a player first, an equal older one dropped, at most
    /// [`KIFU_RECENT_LIMIT`]. Kept even if the settings cannot be written.
    fn ogs_kifu_remember(&mut self, text: &str) {
        let recent = &mut self.config.ogs_kifu.recent;
        recent.retain(|r| r != text);
        recent.insert(0, text.to_string());
        recent.truncate(KIFU_RECENT_LIMIT);
        let _ = self.save_config();
    }

    /// An open request a downloaded record started ended: adopted, failed to read, or replaced by
    /// another open. A download still running for it is not wanted any more.
    pub(super) fn ogs_kifu_open_ended(&mut self, ticket: u64, adopted: bool) {
        let f = &mut self.ogs_kifu;
        if let Some(link) = f.link.take_if(|l| l.ticket == ticket) {
            if let Some(id) = link.download {
                f.tasks.push(OgsKifuTask::Cancel(id));
            }
            return;
        }
        if f.open_ticket != Some(ticket) {
            return;
        }
        f.open_ticket = None;
        if let Some((id, _)) = f.request.take_if(|(_, p)| *p == Purpose::Record(ticket)) {
            f.tasks.push(OgsKifuTask::Cancel(id));
        }
        if adopted {
            f.status = None;
            f.opened += 1;
            return;
        }
        // The read failed (the main window also says why), or another open replaced this one.
        let read_failed = matches!(&self.outcome, Some(OpenOutcome::Failed { ticket: t, .. }) if *t == ticket);
        self.ogs_kifu.status = read_failed.then_some(OgsKifuStatus::Failed {
            failure: OgsFailure::Unavailable,
            record: true,
        });
    }

    pub(super) fn ogs_kifu_view(&self) -> OgsKifuView {
        let f = &self.ogs_kifu;
        let saved = &self.config.ogs_kifu;
        OgsKifuView {
            after_get: saved.after_get.unwrap_or(self.config.fox_kifu.after_get),
            recent: saved.recent.clone(),
            label: f.label.clone().unwrap_or(OgsKifuLabel::Hint),
            rows: f.games.iter().map(row).collect(),
            listed: f.player.is_some(),
            page: f.page.max(1),
            pages: f.pages.max(1),
            shown: f.shown,
            busy: self.ogs_kifu_busy(),
            status: f.status.clone(),
            opened: f.opened,
        }
    }
}

fn row(g: &Game) -> OgsKifuRow {
    OgsKifuRow {
        id: g.id,
        ended: g.ended.clone(),
        black: g.black.clone(),
        white: g.white.clone(),
        result: g.result.clone(),
        size: g.size.clone(),
    }
}

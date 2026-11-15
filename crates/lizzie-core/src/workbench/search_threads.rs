//! 引擎参数 window's 搜索线程数（本次运行） (`SetKataEngines.java:491-528, 608-762, 836-888`,
//! `Leelaz.RuntimeSearchThreads`, `Leelaz.java:20875-21237`): the foreground KataGo process's
//! `numSearchThreads`, read when the window opens, set from it, and restored to the engine's
//! launch source by restarting the engine.
//!
//! The override and the confirmed value belong to one process (the runtime keeps them per
//! instance), so a restart or another engine starts without them. Every step names the process
//! the window was opened for and is stale once another process holds the seat. Reads and sets are
//! refused while something else occupies the engine (a game or match, a lease, ReadBoard GMA, an
//! unrestored engine, a restart or a restore in progress); the runtime fails a request the engine
//! does not answer within 8 s and ignores its late answer.

use super::Workbench;
use crate::api::{
    ApiError, CommandResult, ErrorKind, KataEngineParamsEdit, SearchThreadsCommand, SearchThreadsOperationView,
    SearchThreadsStatus, SearchThreadsView, ThreadRestoreSource,
};
use crate::engine::runtime::{InstanceId, LeaseAvailability, Phase, RuntimeEvent, ThreadOverride};
use crate::engine::tuning::{self, ThreadPolicy, ThreadSource};

/// Java `requestRuntimeSearchThreads`' refusal.
const OCCUPIED: &str = "KataGo process is unavailable or occupied";
const STALE: &str = "SetKataEngines.threadStale";
const READ_FAILED: &str = "SetKataEngines.threadReadFailed";
const SET_FAILED: &str = "SetKataEngines.threadFailed";
const RESTORE_FAILED: &str = "SetKataEngines.threadRestoreFailed";

#[derive(Default)]
pub(super) struct SearchThreadsFlow {
    next_ticket: u64,
    op: Option<Operation>,
}

struct Operation {
    ticket: u64,
    status: SearchThreadsStatus,
    step: Step,
}

enum Step {
    /// Nothing more to do.
    Idle,
    /// Waiting for `process`'s answer to a read.
    Read { process: InstanceId },
    /// Waiting for `process`'s answer to a set; `params` are saved once it confirms.
    Apply { process: InstanceId, params: KataEngineParamsEdit },
    /// A restore of `process` the user is asked to confirm.
    Confirm { process: InstanceId, snapshot: Snapshot, params: KataEngineParamsEdit },
    /// The restarted engine (`None` while it is being started) starts up; then its value is
    /// read (`reading`) and the other rows are saved.
    Restart { successor: Option<InstanceId>, reading: bool, snapshot: Snapshot, params: KataEngineParamsEdit },
}

/// What a restore restarts (Java `RuntimeThreadSourceSnapshot`): the saved engine, its command
/// and thread policy when the restore was offered. A change of any of them refuses it.
struct Snapshot {
    profile_id: String,
    command: String,
    policy: Option<ThreadPolicy>,
}

fn failed(message_key: &str, detail: Option<&str>) -> SearchThreadsStatus {
    SearchThreadsStatus::Failed { message_key: message_key.to_string(), detail: detail.map(str::to_string) }
}

/// Java `Integer.parseInt(text.trim())` within 1–1024.
fn parse_threads(text: &str) -> Option<u32> {
    text.trim().parse::<i64>().ok().filter(|n| (1..=1024).contains(n)).map(|n| n as u32)
}

impl Workbench {
    /// One [`SearchThreadsCommand`] step; see there. A new step replaces a read in progress; any
    /// other operation in progress refuses it.
    pub fn search_threads(&mut self, op: SearchThreadsCommand) -> Result<CommandResult, ApiError> {
        if self.threads.op.as_ref().is_some_and(|o| !matches!(o.step, Step::Idle | Step::Read { .. } | Step::Confirm { .. })) {
            return Err(ApiError::new(ErrorKind::Unavailable, OCCUPIED));
        }
        let ticket = match op {
            SearchThreadsCommand::Read { process } => {
                let ticket = self.threads_begin();
                self.threads_read(ticket, process);
                ticket
            }
            SearchThreadsCommand::Confirm { process, enabled, text, edited, params } => {
                let ticket = self.threads_begin();
                self.threads_confirm(ticket, process, enabled, &text, edited, params);
                ticket
            }
            SearchThreadsCommand::Restore { ticket } => self.threads_restore(ticket),
        };
        self.drain_engine_events();
        Ok(CommandResult::SearchThreads { ticket })
    }

    fn threads_begin(&mut self) -> u64 {
        self.threads.next_ticket += 1;
        let ticket = self.threads.next_ticket;
        self.threads.op = Some(Operation { ticket, status: SearchThreadsStatus::Running, step: Step::Idle });
        ticket
    }

    fn threads_set(&mut self, ticket: u64, status: SearchThreadsStatus, step: Step) {
        if let Some(op) = self.threads.op.as_mut().filter(|o| o.ticket == ticket) {
            op.status = status;
            op.step = step;
        }
    }

    fn threads_finish(&mut self, ticket: u64, status: SearchThreadsStatus) {
        self.threads_set(ticket, status, Step::Idle);
    }

    /// A restore holds analysis from the restart until it ended (Java
    /// `deferResumeUntilCompletion`): the restarted engine is synced, read and given the other rows
    /// before it analyses again as the seat wishes.
    pub(super) fn search_threads_restoring(&self) -> bool {
        self.threads.op.as_ref().is_some_and(|o| matches!(o.step, Step::Restart { .. }))
    }

    fn threads_restore_end(&mut self, ticket: u64, status: SearchThreadsStatus) {
        self.threads_finish(ticket, status);
        self.sync_engine();
    }

    /// `process` is the ready foreground KataGo (Java `RuntimeSearchThreads.isCurrent`).
    fn threads_current(&self, process: InstanceId) -> bool {
        self.engines.foreground() == Some(process)
            && self.engines.phase(process) == Some(Phase::Ready)
            && self.engines.is_katago(process)
    }

    fn threads_remote(&self, process: InstanceId) -> bool {
        self.engines.profile_of(process).is_some_and(|p| p.is_remote_managed())
    }

    /// Whether something else holds the foreground engine (Java's admission: rules mutation and
    /// engine games, ordinary forwarding, ReadBoard GMA, an unrestored engine, a restore). AI 陪练
    /// searches with its own analysis engine, so only what occupies the foreground engine itself
    /// counts during it.
    fn threads_occupied(&self) -> bool {
        let free = |a: LeaseAvailability| matches!(a, LeaseAvailability::Available | LeaseAvailability::MissingCapability);
        self.fox_owns_board()
            || self.fox_holds_engine()
            || self.match_active()
            || self.restart_restoring()
            || self.foreground_lifecycle_held().is_some()
            || match self.engines.lease_availability() {
                LeaseAvailability::HumanSlGame => !free(self.engines.exclusive_gtp_availability()),
                availability => !free(availability),
            }
    }

    fn threads_read(&mut self, ticket: u64, process: InstanceId) {
        if !self.threads_current(process) {
            self.threads_finish(ticket, failed(STALE, None));
        } else if self.threads_occupied() || !self.engines.request_search_threads(process, None, ticket) {
            self.threads_finish(ticket, failed(READ_FAILED, Some(OCCUPIED)));
        } else {
            self.threads_set(ticket, SearchThreadsStatus::Running, Step::Read { process });
        }
    }

    /// `SetKataEngines.confirmSettings`.
    fn threads_confirm(&mut self, ticket: u64, process: InstanceId, enabled: bool, text: &str, edited: bool, params: KataEngineParamsEdit) {
        let Some((state, confirmed)) = self.engines.search_threads(process).filter(|_| self.threads_current(process)) else {
            self.threads_finish(ticket, failed(STALE, None));
            return;
        };
        if !enabled {
            if state != ThreadOverride::On {
                let status = self.threads_save(params, None);
                self.threads_finish(ticket, status);
            } else if self.threads_remote(process) {
                self.threads_finish(ticket, failed("SetKataEngines.threadRemoteRestoreUnsupported", None));
            } else {
                match self.threads_snapshot(process) {
                    Ok((snapshot, source, recommended)) => self.threads_set(
                        ticket,
                        SearchThreadsStatus::ConfirmRestore { source, recommended },
                        Step::Confirm { process, snapshot, params },
                    ),
                    Err(detail) => self.threads_finish(ticket, failed(RESTORE_FAILED, Some(detail))),
                }
            }
            return;
        }
        let Some(threads) = parse_threads(text) else {
            self.threads_finish(ticket, failed("SetKataEngines.threadInvalid", None));
            return;
        };
        if state == ThreadOverride::On && !edited && confirmed == Some(threads) {
            let status = self.threads_save(params, Some(threads));
            self.threads_finish(ticket, status);
        } else if self.threads_occupied() || !self.engines.request_search_threads(process, Some(threads), ticket) {
            self.threads_finish(ticket, failed(SET_FAILED, Some(OCCUPIED)));
        } else {
            self.threads_set(ticket, SearchThreadsStatus::Running, Step::Apply { process, params });
        }
    }

    /// Java `captureRuntimeThreadSourceSnapshot`: the saved local engine `process` runs, and where
    /// a restart takes its threads from: an effective `numSearchThreads` of its command, else its
    /// saved benchmark recommendation (which must be positive), else its configuration file.
    fn threads_snapshot(&self, process: InstanceId) -> Result<(Snapshot, ThreadRestoreSource, Option<u32>), &'static str> {
        let Some(profile) = self.engines.profile_of(process).filter(|p| !p.command.trim().is_empty() && !p.is_remote_managed()) else {
            return Err("Local KataGo launch context is unavailable");
        };
        let Some(entry) = self
            .config
            .engine
            .engines
            .iter()
            .find(|e| e.id == profile.id)
            .filter(|e| e.command == profile.command && !e.is_remote_managed())
        else {
            return Err("Saved KataGo engine entry changed or was removed");
        };
        let policy = entry.thread_policy.as_ref();
        let (source, recommended) = if tuning::has_search_threads(&entry.command) {
            (ThreadRestoreSource::COMMAND, None)
        } else if policy.is_some_and(|p| p.source == ThreadSource::BENCHMARK) {
            (ThreadRestoreSource::BENCHMARK, policy.map(|p| p.threads))
        } else {
            (ThreadRestoreSource::CFG, None)
        };
        if recommended == Some(0) {
            return Err("Saved KataGo benchmark recommendation is invalid");
        }
        let snapshot = Snapshot { profile_id: entry.id.clone(), command: entry.command.clone(), policy: entry.thread_policy.clone() };
        Ok((snapshot, source, recommended))
    }

    /// Java `RuntimeThreadSourceSnapshot.stillCurrent`: the saved engine still has that command
    /// and policy and is still local.
    fn threads_snapshot_current(&self, snapshot: &Snapshot) -> bool {
        self.config.engine.engines.iter().any(|e| {
            e.id == snapshot.profile_id
                && e.command == snapshot.command
                && !e.is_remote_managed()
                && e.thread_policy == snapshot.policy
        })
    }

    /// `RuntimeThreadSourceRestore.execute`: the restore `ticket` offered, once. The engine is
    /// ended and started again through the restart of a failed foreground engine; the new
    /// process's value is read and the other rows are saved once it is ready
    /// ([`Self::search_threads_settle`]). Returns the operation's ticket.
    fn threads_restore(&mut self, ticket: u64) -> u64 {
        let offered = self.threads.op.as_mut().filter(|o| o.ticket == ticket && matches!(o.step, Step::Confirm { .. }));
        let Some(Step::Confirm { process, snapshot, params }) = offered.map(|o| std::mem::replace(&mut o.step, Step::Idle)) else {
            let ticket = self.threads_begin();
            self.threads_finish(ticket, failed(RESTORE_FAILED, Some("Source restore already executed")));
            return ticket;
        };
        let target = self.threads_current(process)
            && self.engines.search_threads(process).is_some_and(|(state, _)| state == ThreadOverride::On)
            && !self.threads_remote(process)
            && self.engines.profile_of(process).is_some_and(|p| p.id == snapshot.profile_id && p.command == snapshot.command)
            && self.threads_snapshot_current(&snapshot)
            && !self.benchmark_running();
        if !target {
            self.threads_finish(ticket, failed(RESTORE_FAILED, Some("KataGo source restore target changed or is unavailable")));
            return ticket;
        }
        if self.threads_occupied() {
            self.threads_finish(ticket, failed(RESTORE_FAILED, Some("KataGo engine is occupied")));
            return ticket;
        }
        let profile_id = snapshot.profile_id.clone();
        self.threads_set(ticket, SearchThreadsStatus::Running, Step::Restart { successor: None, reading: false, snapshot, params });
        self.engines.terminate(process);
        if !self.restart_failed_foreground(&profile_id) {
            self.threads_restore_end(ticket, failed(RESTORE_FAILED, Some("KataGo restart successor is unavailable")));
            return ticket;
        }
        let successor = self.engines.foreground();
        if let Some(Operation { step: Step::Restart { successor: slot, .. }, .. }) = self.threads.op.as_mut().filter(|o| o.ticket == ticket) {
            *slot = successor;
        }
        ticket
    }

    /// Saves the other rows (`applyExistingSettings`); `threads` is what the engine confirmed.
    fn threads_save(&mut self, params: KataEngineParamsEdit, threads: Option<u32>) -> SearchThreadsStatus {
        match self.save_kata_engine_params(params) {
            Ok(None) => SearchThreadsStatus::Done { threads },
            Ok(Some(key)) => failed(key, None),
            Err(e) => failed(SET_FAILED, Some(&e.message)),
        }
    }

    /// Takes the answers to this module's requests; the answer of an earlier operation only
    /// recorded what the process holds.
    pub(super) fn search_threads_event(&mut self, ev: RuntimeEvent) -> Option<RuntimeEvent> {
        let RuntimeEvent::SearchThreads { instance, ticket, result } = ev else { return Some(ev) };
        let Some(op) = self.threads.op.as_mut().filter(|o| o.ticket == ticket) else { return None };
        match std::mem::replace(&mut op.step, Step::Idle) {
            Step::Read { process } if process == instance => {
                let status = match result {
                    _ if !self.threads_current(process) => failed(STALE, None),
                    Ok(threads) => SearchThreadsStatus::Done { threads: Some(threads) },
                    Err(detail) => failed(READ_FAILED, Some(&detail)),
                };
                self.threads_finish(ticket, status);
            }
            Step::Apply { process, params } if process == instance => {
                let status = match result {
                    _ if !self.threads_current(process) => failed(STALE, None),
                    Ok(threads) => self.threads_save(params, Some(threads)),
                    Err(detail) => failed(SET_FAILED, Some(&detail)),
                };
                self.threads_finish(ticket, status);
            }
            // The new process's value: a failed read does not stop the restore.
            Step::Restart { successor: Some(successor), reading: true, snapshot, params } if successor == instance => {
                self.threads_restored(ticket, successor, &snapshot, params, result.ok());
            }
            step => op_restore(self, ticket, step),
        }
        None
    }

    /// The end of a restore: the new process holds the seat and the saved source is unchanged
    /// (`beforeResume`, then `resumeClosedEngineAfterBoardSynchronization`); the other rows are
    /// saved and the engine analyses as the seat wishes.
    fn threads_restored(&mut self, ticket: u64, successor: InstanceId, snapshot: &Snapshot, params: KataEngineParamsEdit, threads: Option<u32>) {
        let status = if self.engines.foreground() != Some(successor) {
            failed(RESTORE_FAILED, Some("KataGo restart successor changed"))
        } else if !self.threads_snapshot_current(snapshot) {
            failed(RESTORE_FAILED, Some("KataGo source changed during parameter handoff"))
        } else {
            self.threads_save(params, threads)
        };
        self.threads_restore_end(ticket, status);
    }

    /// A step whose process ended or left the seat is stale; a restarted engine that became ready
    /// is asked for its value (it ignores the occupancy of its own restore), one that failed or
    /// was replaced ends the restore.
    pub(super) fn search_threads_settle(&mut self) -> bool {
        let Some(op) = self.threads.op.as_ref() else { return false };
        let ticket = op.ticket;
        match &op.step {
            Step::Read { process } | Step::Apply { process, .. } | Step::Confirm { process, .. } => {
                let process = *process;
                if self.threads_current(process) {
                    return false;
                }
                let key = if matches!(op.step, Step::Confirm { .. }) { RESTORE_FAILED } else { STALE };
                let detail = matches!(op.step, Step::Confirm { .. }).then_some("KataGo process or foreground selection changed");
                self.threads_finish(ticket, failed(key, detail));
                true
            }
            Step::Restart { successor: Some(successor), reading, .. } => {
                let (successor, reading) = (*successor, *reading);
                if self.engines.foreground() != Some(successor) {
                    self.threads_restore_end(ticket, failed(RESTORE_FAILED, Some("KataGo restart successor is unavailable")));
                    return true;
                }
                if reading || self.engines.phase(successor) != Some(Phase::Ready) {
                    return false;
                }
                if self.engines.request_search_threads(successor, None, ticket) {
                    if let Some(Operation { step: Step::Restart { reading, .. }, .. }) = self.threads.op.as_mut() {
                        *reading = true;
                    }
                } else if let Some(Step::Restart { snapshot, params, .. }) =
                    self.threads.op.as_mut().map(|o| std::mem::replace(&mut o.step, Step::Idle))
                {
                    self.threads_restored(ticket, successor, &snapshot, params, None);
                }
                true
            }
            Step::Idle | Step::Restart { successor: None, .. } => false,
        }
    }

    pub(super) fn search_threads_view(&self) -> Option<SearchThreadsView> {
        let process = self.engines.foreground().filter(|&f| self.engines.phase(f) == Some(Phase::Ready))?;
        let (state, confirmed) = self.engines.search_threads(process)?;
        Some(SearchThreadsView { process, state, confirmed, remote: self.threads_remote(process) })
    }

    pub(super) fn search_threads_operation_view(&self) -> Option<SearchThreadsOperationView> {
        self.threads.op.as_ref().map(|o| SearchThreadsOperationView { ticket: o.ticket, status: o.status.clone() })
    }
}

/// An answer that does not match the operation's step leaves the step as it was.
fn op_restore(wb: &mut Workbench, ticket: u64, step: Step) {
    if let Some(op) = wb.threads.op.as_mut().filter(|o| o.ticket == ticket) {
        op.step = step;
    }
}

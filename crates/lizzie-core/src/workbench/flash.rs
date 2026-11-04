//! 闪电分析 (Java `LizzieFrame.flashAnalyzeGame`, `AnalysisEngine.startRequest` /
//! `startRequestAllBranches`, `WaitForAnalysis`): the user analyses the main trunk (整盘速览), a
//! range of its moves (局部精析) or every branch (全部分支精析) with the 精析设置 analysis engine —
//! a KataGo analysis process, or the borrowed foreground engine when 复用当前前台引擎 is set.
//!
//! A run asks once for every chosen move that has fewer visits than a request searches (all of
//! them with 总是覆盖), in order, and stores each answer where it was asked for, as automatic quick
//! analysis does. While an analysis process runs, the foreground engine stops; it analyses again
//! when the run completes (`shouldRePonder`), and the user's resume lets it analyse at once.
//! 取消分析 keeps every answer so far; the cancelled process ends and the foreground engine stays
//! stopped, as Java's `destroyAnalysisEngine` leaves it. A borrowed engine is given back through
//! the foreground lease, which only restores the engine it took: a run is complete once the engine
//! is confirmed back, and a failed restore is reported and leaves the engine unrestored.
//!
//! An automatic quick analysis hands its engine over first (`MANUAL_FLASH`): its analysis process
//! stays for the flash run, a borrowed engine is given back before the run borrows it again, and
//! the foreground engine does not resume in between. Opening another record or a new board ends a
//! run silently (`requestStillTargetsCurrentGame`); a switched engine ends a borrowing run.

use std::collections::{HashMap, VecDeque};

use super::analysis::AnalysisStore;
use super::quick::{Query, RulesSignature};
use super::Workbench;
use crate::api::{ApiError, FlashAnalysisView, FlashOutcome, FlashTarget};
use crate::browse::Navigation;
use crate::engine::kata_analysis::{self, Response};
use crate::engine::runtime::{EngineFailure, InstanceId, LeaseAvailability, LeaseFailure, RuntimeEvent};
use crate::record::{GameRecord, NodeId, ROOT};
use crate::settings::FlashRange;

/// `LizzieFrame.analysisCommandMissing`: no analysis command and the current engine not reused.
pub const ANALYSIS_COMMAND_MISSING: &str = "LizzieFrame.analysisCommandMissing";
/// `AnalysisEngine.requestDispatchFailed`: the borrowed engine refused or lost a search.
pub(super) const REQUEST_DISPATCH_FAILED: &str = "AnalysisEngine.requestDispatchFailed";
/// `AnalysisEngine.foregroundRestoreFailed`: the borrowed engine could not be given back.
pub(super) const FOREGROUND_RESTORE_FAILED: &str = "AnalysisEngine.foregroundRestoreFailed";
/// `WholeGameAnalysis.conflict.analysis`: a whole-game session holds the analysis entries.
pub const WHOLE_GAME_CONFLICT: &str = "WholeGameAnalysis.conflict.analysis";

#[derive(Default)]
pub(super) struct FlashFlow {
    run: Option<Run>,
    next_run: u64,
    /// The last flash analysis asked for, for 确定并重新计算 (`analysisRecentIsPartGame`,
    /// `analysisRecentIsAllBranches`).
    recent: Option<FlashTarget>,
    /// A run waiting for a borrowed engine to come back before it starts (`whenSettled`), with
    /// the positions it was accepted for.
    waiting: Option<(u64, Vec<(NodeId, u64)>)>,
    /// A cancelled run's borrowed engine still being given back: run and lease.
    releasing: Option<(u64, u64)>,
    outcome: Option<FlashOutcome>,
}

struct Run {
    id: u64,
    doc: u64,
    total: u32,
    done: u32,
    /// An analysis process took the position from the analysing foreground engine, which
    /// analyses again when the run completes (`shouldRePonder`).
    holds: bool,
    /// The rules settings the run asks under; answers under others are dropped.
    rules: RulesSignature,
    engine: Engine,
}

enum Engine {
    Process {
        instance: InstanceId,
        command: String,
        queries: HashMap<String, Query>,
    },
    Lease {
        lease: u64,
        /// The positions still to search: node and the stamp it had when the run was asked for.
        queue: VecDeque<(NodeId, u64)>,
        /// Giving the engine back after the last search (`true`) or a failed one.
        ending: Option<bool>,
    },
}

/// `AnalysisSettings.reuseStatus.<availability>` (`Leelaz.ExclusiveGtpLeaseAvailability`).
pub(super) fn reuse_status_key(availability: LeaseAvailability) -> String {
    let name = match availability {
        LeaseAvailability::Available => "available",
        LeaseAvailability::NoForegroundEngine => "no_foreground_engine",
        LeaseAvailability::EngineNotReady => "engine_not_ready",
        LeaseAvailability::NotKatago => "not_katago",
        LeaseAvailability::MissingCapability => "missing_capability",
        LeaseAvailability::ExistingLease => "existing_lease",
        LeaseAvailability::EngineStateUnrestored => "engine_state_unrestored",
        LeaseAvailability::PlayMode => "play_mode",
        LeaseAvailability::HumanSlGame => "human_sl_game",
    };
    format!("AnalysisSettings.reuseStatus.{name}")
}

impl Workbench {
    // -------------------------------------------------------------------- state

    /// A flash analysis is running or waiting to start.
    pub(super) fn flash_active(&self) -> bool {
        self.flash.run.is_some() || self.flash.waiting.is_some()
    }

    /// A cancelled run's borrowed engine is still being given back.
    pub(super) fn flash_releasing(&self) -> bool {
        self.flash.releasing.is_some()
    }

    /// The foreground engine leaves the position to a flash analysis: a waiting or borrowing run,
    /// or an analysis process that took it from the analysing foreground engine.
    pub(super) fn flash_holds_foreground(&self) -> bool {
        self.flash.waiting.is_some()
            || self
                .flash
                .run
                .as_ref()
                .is_some_and(|r| r.holds || matches!(r.engine, Engine::Lease { .. }))
    }

    /// The moves a run asks about, in order (see [`flash_nodes_of`]).
    fn flash_nodes(&self, target: FlashTarget) -> Vec<NodeId> {
        let a = &self.config.analysis_engine;
        let visits = u64::from(a.max_visits.max(1)) + 1;
        flash_nodes_of(&self.doc.record, &self.store, target, visits, a.always_override)
    }

    /// The positions a run is asked for, frozen when it is accepted: each node with its stamp, so
    /// a later edit or deletion makes it a position the run no longer searches.
    fn flash_plan(&self, target: FlashTarget) -> Vec<(NodeId, u64)> {
        let rec = &self.doc.record;
        self.flash_nodes(target)
            .into_iter()
            .map(|n| (n, rec.node(n).stamp))
            .collect()
    }

    // -------------------------------------------------------------------- operations

    /// 整盘速览 / 局部精析 / 全部分支精析 (`flashAnalyzeGame`). `Err` holds the Java key of a refusal.
    pub fn start_flash_analysis(&mut self, target: FlashTarget) -> Result<Result<bool, String>, ApiError> {
        if self.whole_game_active() {
            return Ok(Err(WHOLE_GAME_CONFLICT.to_string()));
        }
        // A batch holds the analysis engine or the foreground engine for its own records.
        if self.batch_engaged() {
            return Ok(Err(super::batch::CONFLICT.to_string()));
        }
        if let FlashTarget::MoveRange { start_move, end_move } = target {
            let range = FlashRange { start_move, end_move };
            if self.config.flash_range != range {
                self.commit_config(|c| c.flash_range = range)?;
            }
        }
        let borrow = self.config.analysis_engine.reuse_current_engine;
        if !borrow && self.analysis_command_base(false).trim().is_empty() {
            return Ok(Err(ANALYSIS_COMMAND_MISSING.to_string()));
        }
        // `cancelPendingManualAutoAnalysisForExclusiveTask`.
        self.auto_analysis_cancel_pending();
        self.flash.recent = Some(target);
        self.flash_replace();
        // `MANUAL_FLASH`: an automatic quick analysis hands its engine over.
        self.quick_hand_over();
        self.flash.next_run += 1;
        let id = self.flash.next_run;
        let plan = self.flash_plan(target);
        if self.quick_releasing() || self.flash_releasing() {
            self.flash.waiting = Some((id, plan));
            self.sync_engine();
            return Ok(Ok(true));
        }
        Ok(self.flash_begin(id, plan).map(|()| true))
    }

    /// 取消分析 (`destroyAnalysisEngine`): the answers so far stay. An analysis process ends and
    /// the foreground engine stays stopped; a borrowed engine is given back.
    pub fn cancel_flash_analysis(&mut self) -> bool {
        if self.flash.waiting.take().is_some() {
            self.sync_engine();
            return true;
        }
        let Some(run) = self.flash.run.take() else {
            return false;
        };
        match run.engine {
            Engine::Process { instance, .. } => {
                self.engines.terminate(instance);
                if run.holds {
                    self.paused = true;
                }
            }
            Engine::Lease { lease, ending, .. } => self.flash_give_back(run.id, lease, ending.is_none()),
        }
        self.drain_engine_events();
        self.sync_engine();
        true
    }

    /// 确定并重新计算: the run stops and the last flash analysis starts again.
    pub fn redo_flash_analysis(&mut self) -> Result<Result<bool, String>, ApiError> {
        let Some(target) = self.flash.recent else {
            return Ok(Ok(false));
        };
        self.cancel_flash_analysis();
        self.start_flash_analysis(target)
    }

    /// Starts run `id` now. `Err` holds the Java key of why it cannot.
    fn flash_begin(&mut self, id: u64, plan: Vec<(NodeId, u64)>) -> Result<(), String> {
        let borrow = self.config.analysis_engine.reuse_current_engine;
        if borrow {
            let availability = self.engines.lease_availability();
            if availability != LeaseAvailability::Available {
                self.sync_engine();
                return Err(reuse_status_key(availability));
            }
        }
        // Positions edited or removed since the run was asked for are not searched.
        let rec = self.doc.record.clone();
        let plan: Vec<(NodeId, u64)> = plan
            .into_iter()
            .filter(|&(n, stamp)| rec.contains(n) && rec.node(n).stamp == stamp)
            .collect();
        let nodes: Vec<NodeId> = plan.iter().map(|&(n, _)| n).collect();
        if nodes.is_empty() {
            // `finishSuccessfulEmptyRequest`: nothing to ask, nothing to report.
            self.sync_engine();
            return Ok(());
        }
        let total = nodes.len() as u32;
        let rules = self.quick_rules_signature();
        let engine = if borrow {
            let lease = self.engines.begin_lease().map_err(reuse_status_key)?;
            Engine::Lease {
                lease,
                queue: plan.into(),
                ending: None,
            }
        } else {
            let command = self.tuned_analysis_command(&self.analysis_command_base(false));
            let instance = match self.take_analysis_process(&command) {
                Ok(instance) => instance,
                Err(message) => {
                    self.flash_engine_failed(id, &command, true, message, Vec::new());
                    self.sync_engine();
                    return Ok(());
                }
            };
            let (queries, sent) = self.send_analysis_queries(instance, &nodes, &format!("flash{id}"));
            if !sent {
                self.engines.terminate(instance);
                self.flash_engine_failed(id, &command, true, "input closed".into(), Vec::new());
                self.sync_engine();
                return Ok(());
            }
            Engine::Process {
                instance,
                command,
                queries,
            }
        };
        self.flash.run = Some(Run {
            id,
            doc: self.doc.id,
            total,
            done: 0,
            holds: !borrow && !self.paused,
            rules,
            engine,
        });
        self.drain_engine_events();
        self.sync_engine();
        Ok(())
    }

    /// A new run replaces the current one (`prepareRequestState`): its analysis process stays for
    /// reuse, its borrowed engine is given back first.
    fn flash_replace(&mut self) {
        self.flash.waiting = None;
        let Some(run) = self.flash.run.take() else { return };
        match run.engine {
            Engine::Process { instance, command, .. } => self.keep_analysis_process(instance, command),
            Engine::Lease { lease, ending, .. } => self.flash_give_back(run.id, lease, ending.is_none()),
        }
        self.drain_engine_events();
    }

    /// Gives a cancelled run's borrowed engine back and waits for it.
    fn flash_give_back(&mut self, run: u64, lease: u64, end: bool) {
        if end {
            self.engines.end_lease(lease);
        }
        if self.engines.lease_alive(lease) {
            self.flash.releasing = Some((run, lease));
        }
    }

    /// The record is being replaced (another record, a new board): a run of it ends without a
    /// report (`requestStillTargetsCurrentGame`). An analysis process ends; a borrowed engine is
    /// given back.
    pub(super) fn flash_document_closing(&mut self) {
        self.flash.waiting = None;
        let Some(run) = self.flash.run.take() else { return };
        match run.engine {
            Engine::Process { instance, .. } => self.engines.terminate(instance),
            Engine::Lease { lease, ending, .. } => self.flash_give_back(run.id, lease, ending.is_none()),
        }
        self.drain_engine_events();
    }

    /// The foreground engine was switched, restarted or closed: a borrowing run ends and gives
    /// the engine it took back; an analysis process continues and no longer holds the position.
    pub(super) fn flash_engine_changed(&mut self) {
        match self.flash.run.as_mut() {
            Some(Run {
                engine: Engine::Lease { .. },
                ..
            }) => self.flash_document_closing(),
            Some(run) => run.holds = false,
            None => {}
        }
    }

    /// Space / 继续分析 while an analysis process holds the position: the foreground engine
    /// analyses again at once (`resumeFromAnalysisControl`). Returns whether it was handled here.
    pub(super) fn flash_toggle_pause(&mut self) -> bool {
        if self.paused {
            return false;
        }
        match self.flash.run.as_mut() {
            Some(run) if run.holds => {
                run.holds = false;
                true
            }
            _ => false,
        }
    }

    /// A borrowed engine came back (quick analysis or a cancelled run): a read waiting for it is
    /// adopted, then a run waiting for it starts.
    pub(super) fn borrowed_engine_returned(&mut self) {
        self.complete_deferred_open();
        if self.quick_releasing() || self.flash_releasing() {
            return;
        }
        if let Some((id, plan)) = self.flash.waiting.take() {
            if let Err(message_key) = self.flash_begin(id, plan) {
                self.flash.outcome = Some(FlashOutcome::Failed { run: id, message_key });
            }
        }
    }

    // -------------------------------------------------------------------- ending

    /// Every position was answered (`setResult`): browsing at the root steps to the first move,
    /// and the foreground engine analyses again unless the user paused it.
    fn flash_completed(&mut self, run: &Run) {
        if run.doc == self.doc.id && self.cursor == ROOT {
            self.navigate(Navigation::Forward(1));
        }
        self.flash.outcome = Some(FlashOutcome::Completed { run: run.id });
    }

    /// The analysis process failed (`showErrMsg`): reported like an engine failure, then
    /// 精析设置 offers 确定并重新计算.
    fn flash_engine_failed(
        &mut self,
        run: u64,
        command: &str,
        during_startup: bool,
        message: String,
        output: Vec<String>,
    ) {
        self.engine_failed(EngineFailure {
            instance: 0,
            profile_id: String::new(),
            engine_name: "AnalysisEngine".into(),
            command: command.to_string(),
            during_startup,
            message,
            output,
        });
        self.flash.outcome = Some(FlashOutcome::EngineFailed { run });
    }

    /// An answer (or an error) for one query of the running process.
    fn flash_worker_line(&mut self, line: &str) {
        let size = self.doc.record.size;
        let signature = self.quick_rules_signature();
        let Some(run) = self.flash.run.as_mut() else { return };
        let Engine::Process { queries, .. } = &mut run.engine else {
            return;
        };
        let (query, line) = match kata_analysis::parse(line, size) {
            Response::Result { id, line } => match queries.remove(&id) {
                Some(q) => (q, line),
                None => return,
            },
            Response::Error { id, message } => {
                let ours = id.is_some_and(|id| queries.contains_key(&id));
                self.log(true, format!("flash analysis error: {message}"));
                if ours {
                    self.flash_request_failed();
                }
                return;
            }
            Response::Other => return,
        };
        let run = self.flash.run.as_mut().unwrap();
        run.done += 1;
        let (doc, same_rules) = (run.doc, run.rules == signature);
        if same_rules {
            self.store_background_result(doc, &query, line, false);
        }
        let finished =
            matches!(&self.flash.run, Some(Run { engine: Engine::Process { queries, .. }, .. }) if queries.is_empty());
        if finished {
            let run = self.flash.run.take().unwrap();
            let Engine::Process { instance, command, .. } = &run.engine else {
                unreachable!()
            };
            // `analysis-auto-quit`; otherwise the process stays for the next analysis.
            if self.config.analysis_engine.auto_quit {
                self.engines.terminate(*instance);
            } else {
                self.keep_analysis_process(*instance, command.clone());
            }
            self.flash_completed(&run);
            self.sync_engine();
        }
    }

    /// The process rejected a query (`finishFailedRequestDispatch`): the run ends with
    /// requestDispatchFailed, its answers so far stay, the process stays for reuse and the
    /// foreground engine analyses again unless the user paused it.
    fn flash_request_failed(&mut self) {
        let run = self.flash.run.take().unwrap();
        if let Engine::Process { instance, command, .. } = run.engine {
            self.keep_analysis_process(instance, command);
        }
        self.flash.outcome = Some(FlashOutcome::Failed {
            run: run.id,
            message_key: REQUEST_DISPATCH_FAILED.into(),
        });
        self.sync_engine();
    }

    /// Asks the borrowed engine about the next move, or gives it back after the last one.
    fn flash_lease_next(&mut self) {
        loop {
            let Some(Run {
                engine:
                    Engine::Lease {
                        lease,
                        queue,
                        ending: None,
                    },
                done,
                ..
            }) = self.flash.run.as_mut()
            else {
                return;
            };
            let lease = *lease;
            let Some((node, stamp)) = queue.pop_front() else {
                return self.flash_lease_end(true);
            };
            let rec = &self.doc.record;
            if !rec.contains(node) || rec.node(node).stamp != stamp {
                *done += 1;
                continue;
            }
            let mut target = self.analysis_target();
            target.node = node;
            let rules = self.quick_lease_rules();
            let visits = u64::from(self.config.analysis_engine.max_visits.max(1)) + 1;
            if !self.engines.lease_search(lease, &target, visits, rules.as_deref()) {
                return self.flash_lease_end(false);
            }
            return;
        }
    }

    /// The run is over (`completed` or failed): its borrowed engine is given back first.
    fn flash_lease_end(&mut self, completed: bool) {
        let Some(Run {
            engine: Engine::Lease { lease, ending, .. },
            ..
        }) = self.flash.run.as_mut()
        else {
            return;
        };
        if ending.is_some() {
            return;
        }
        *ending = Some(completed);
        let lease = *lease;
        self.engines.end_lease(lease);
        self.drain_engine_events();
    }

    /// Runtime events of flash analysis; any other event is handed back.
    pub(super) fn flash_event(&mut self, ev: RuntimeEvent) -> Option<RuntimeEvent> {
        let (lease, process) = match self.flash.run.as_ref().map(|r| &r.engine) {
            Some(Engine::Lease { lease, .. }) => (Some(*lease), None),
            Some(Engine::Process { instance, .. }) => (None, Some(*instance)),
            None => (None, None),
        };
        let releasing = self.flash.releasing.map(|(_, l)| l);
        match ev {
            RuntimeEvent::LeaseReady { lease: l } if lease == Some(l) => self.flash_lease_next(),
            RuntimeEvent::LeaseResult {
                lease: l,
                doc,
                node,
                stamp,
                komi,
                rules_rev,
                to_play,
                line,
            } if lease == Some(l) => {
                let signature = self.quick_rules_signature();
                let run = self.flash.run.as_mut().unwrap();
                if matches!(run.engine, Engine::Lease { ending: None, .. }) {
                    run.done += 1;
                    if run.rules == signature {
                        let q = Query {
                            node,
                            stamp,
                            komi,
                            rules_rev,
                            to_play,
                        };
                        self.store_background_result(doc, &q, line, true);
                    }
                    self.flash_lease_next();
                }
            }
            RuntimeEvent::LeaseSearchFailed { lease: l } if lease == Some(l) => self.flash_lease_end(false),
            RuntimeEvent::LeaseEnded { lease: l, failure } if lease == Some(l) || releasing == Some(l) => {
                if let Some(f) = failure {
                    self.log(true, format!("foreground engine lease {l} ended: {f:?}"));
                }
                // A closed engine is reported as an engine failure, not as a failed restore.
                let restore_failed = failure.is_some_and(|f| f.unrestores() && f != LeaseFailure::TransportClosed);
                if releasing == Some(l) {
                    let (run, _) = self.flash.releasing.take().unwrap();
                    if restore_failed {
                        self.flash.outcome = Some(FlashOutcome::Failed {
                            run,
                            message_key: FOREGROUND_RESTORE_FAILED.into(),
                        });
                    }
                    self.borrowed_engine_returned();
                } else {
                    let run = self.flash.run.take().unwrap();
                    let completed = matches!(run.engine, Engine::Lease { ending: Some(true), .. });
                    if restore_failed {
                        self.flash.outcome = Some(FlashOutcome::Failed {
                            run: run.id,
                            message_key: FOREGROUND_RESTORE_FAILED.into(),
                        });
                    } else if completed && failure.is_none() {
                        self.flash_completed(&run);
                    } else {
                        // A refused or lost search, or an end the run did not ask for.
                        self.flash.outcome = Some(FlashOutcome::Failed {
                            run: run.id,
                            message_key: REQUEST_DISPATCH_FAILED.into(),
                        });
                    }
                }
                self.sync_engine();
            }
            RuntimeEvent::WorkerLine { instance, line } if process == Some(instance) => self.flash_worker_line(&line),
            RuntimeEvent::WorkerExited {
                instance,
                message,
                output,
            } if process == Some(instance) => {
                let run = self.flash.run.take().unwrap();
                // `finishAbortedAnalysis` does not resume the foreground engine.
                if run.holds {
                    self.paused = true;
                }
                if let Engine::Process { command, .. } = &run.engine {
                    self.flash_engine_failed(run.id, command, false, message, output);
                }
                self.sync_engine();
            }
            other => return Some(other),
        }
        None
    }

    // -------------------------------------------------------------------- projection

    pub(super) fn flash_view(&self) -> Option<FlashAnalysisView> {
        if let Some((run, _)) = &self.flash.waiting {
            return Some(FlashAnalysisView {
                run: *run,
                loading: true,
                done: 0,
                total: 0,
                borrowed: self.config.analysis_engine.reuse_current_engine,
            });
        }
        self.flash.run.as_ref().map(|r| FlashAnalysisView {
            run: r.id,
            loading: false,
            done: r.done,
            total: r.total,
            borrowed: matches!(r.engine, Engine::Lease { .. }),
        })
    }

    /// The flash outcome for the next view, handed over once.
    pub(super) fn take_flash_outcome(&mut self) -> Option<FlashOutcome> {
        self.flash.outcome.take()
    }

    pub(super) fn flash_outcome(&self) -> Option<FlashOutcome> {
        self.flash.outcome.clone()
    }
}

/// The moves a flash request asks about, in order: main-trunk moves and passes counted from 1
/// within `[start, end)` (`shouldAnalyzeTurn`), or every move of every branch, depth first with
/// the main line first (`startRequestAllBranchesNow`); moves with `visits` already are skipped
/// unless 总是覆盖 (`shouldAnalyzeNode`).
pub(super) fn flash_nodes_of(
    rec: &GameRecord,
    store: &AnalysisStore,
    target: FlashTarget,
    visits: u64,
    always_override: bool,
) -> Vec<NodeId> {
    let wanted = |n: NodeId| {
        rec.node(n).kind.is_action() && (always_override || store.get(rec, n).map_or(0, |r| r.visits) < visits)
    };
    let (start, end) = match target {
        FlashTarget::AllBranches => {
            let mut nodes = Vec::new();
            let mut stack = vec![ROOT];
            while let Some(n) = stack.pop() {
                if wanted(n) {
                    nodes.push(n);
                }
                stack.extend(rec.node(n).children.iter().rev().filter(|&&c| rec.contains(c)));
            }
            return nodes;
        }
        FlashTarget::WholeGame => (-1, -1),
        FlashTarget::MoveRange { start_move, end_move } => (start_move, end_move),
    };
    rec.path_to(rec.line_end(ROOT))
        .into_iter()
        .filter(|&n| rec.node(n).kind.is_action())
        .zip(1..)
        .filter(|&(_, number)| (start < 0 || number >= start) && (end < 0 || number < end))
        .map(|(n, _)| n)
        .filter(|&n| wanted(n))
        .collect()
}

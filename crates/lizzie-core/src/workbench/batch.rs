//! 批量分析 / 批量精析 (Java `LizzieFrame.openFileWithAna`, `openOrdinaryBatchAnalysis`,
//! `flashAnalyzeGameBatch`, `flashAutoAnaSaveAndLoad`, `BottomToolbar.autoAnaSaveAndLoad`,
//! `AnalysisTable`).
//!
//! The batch owns its queue (Java `Batchfiles` from `BatchAnaNum`: the first row is the current
//! record) and the record it analyses. That record is a background document with its own
//! identity, analysis store, komi and rules: the open record is never replaced, moved, written or
//! marked by the batch, unlike Java, which loads every queued record into the main board. Each
//! queued file is read off the workbench lock like an open, analysed, then saved beside its
//! source as `<name>_<已分析>_<yyyyMMddHHmmss>.sgf` through the one serial writer. The first
//! record is read before the settings dialog, as Java loads it first.
//!
//! - 批量分析 runs the automatic analysis ([`super::auto_analysis`]) of each record with the
//!   foreground engine; the foreground engine follows that record while it runs. A natural end
//!   saves the record and loads the next; a forced stop saves the current record and ends the
//!   batch; either end forgets the queue (`endBatchAutoAnalysis`).
//! - 批量精析 is a flash analysis of each record (`max(2, batch-analysis-playouts)` visits) with
//!   the 精析设置 analysis process, or the borrowed foreground engine, kept for the whole batch.
//!   取消分析 ends the engine without saving the current record and keeps the queue at it, with
//!   the record and its results, so the batch can continue from 进度表.
//!
//! A record that cannot be read ends 批量分析 with that file named (Java's ordinary batch ends at
//! a load failure); 批量精析 skips it (Java would analyse the previous board under the bad file's
//! name). A failed save is reported with the batch's end instead of being dropped (Java prints
//! the stack trace and reports success). A new batch waits until the previous batch's copies are
//! written, so each end reports its own failures.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::analysis::{fill_pv_captures, node_analysis, AnalysisStore};
use super::auto_analysis::{analyzed_output, check_auto_settings};
use super::flash::{
    flash_nodes_of, reuse_status_key, ANALYSIS_COMMAND_MISSING, FOREGROUND_RESTORE_FAILED, REQUEST_DISPATCH_FAILED,
    WHOLE_GAME_CONFLICT,
};
use super::quick::Query;
use super::{PendingSave, SaveJob, Workbench, FAILED_TO_OPEN_FILE, SAVE_FILE_FAILED};
use crate::api::{
    ApiError, BatchCommand, BatchFailure, BatchItemView, BatchMode, BatchMove, BatchNotice, BatchRunView, BatchView,
    ErrorKind, FlashTarget, MoveChange, MoveEvaluation, OpenSource,
};
use crate::engine::gtp::InfoLine;
use crate::engine::kata_analysis::{self, Response};
use crate::engine::rules::KataRules;
use crate::engine::runtime::{EngineFailure, InstanceId, LeaseFailure, RuntimeEvent, Target};
use crate::formats::{saved_analysis, ReadError};
use crate::record::{DocumentEdit, GameRecord, NodeId, RecordFormat, Stone, ROOT};
use crate::settings::AutoAnalysisSettings;

/// `AutoAnalyze.conflict`: another analysis task runs.
pub(super) const CONFLICT: &str = "AutoAnalyze.conflict";
const COMPLETE: &str = "Leelaz.batchAutoAnalyzeComplete";
const STOPPED: &str = "Leelaz.batchAutoAnalyzeStopped";
/// The `AP` written into analysed copies, as automatic analysis writes it.
const APPLICATION: &str = "LizzieYzy Tauri";

struct Item {
    id: u64,
    path: String,
    /// Loading it records the recent list and last folder (`loadFile(file, fromTemp = false)`).
    remember: bool,
}

/// A queued record being analysed: a background document of the batch.
pub(super) struct BatchDoc {
    id: u64,
    path: String,
    record: Arc<GameRecord>,
    store: AnalysisStore,
    komi: f64,
    rules: Option<KataRules>,
    cursor: NodeId,
}

#[derive(Clone, Copy)]
struct FlashParams {
    start_move: i32,
    end_move: i32,
    visits: u32,
    all_branches: bool,
}

enum Work {
    Ordinary(AutoAnalysisSettings),
    Flash(FlashParams),
}

enum FlashEngine {
    Process {
        instance: InstanceId,
        command: String,
        queries: HashMap<String, Query>,
    },
    Lease {
        lease: u64,
        ready: bool,
        queue: VecDeque<NodeId>,
        /// Given back: the batch's end notice once the engine is confirmed back.
        ending: Option<Option<&'static str>>,
    },
}

struct Run {
    work: Work,
    /// 批量精析 borrows the foreground engine (复用当前前台引擎 when the batch started); the route
    /// stays for the whole batch.
    borrow: bool,
    item: u64,
    /// The read of the current record in flight.
    read: Option<u64>,
    doc: Option<BatchDoc>,
    /// 批量精析's engine, kept from record to record.
    engine: Option<FlashEngine>,
    /// 批量精析's analysis process took the position from the analysing foreground engine.
    holds: bool,
    done: u32,
    total: u32,
}

#[derive(Default)]
pub(super) struct BatchFlow {
    queue: VecDeque<Item>,
    next_item: u64,
    setup: Option<BatchMode>,
    /// The first record's read before its settings dialog (`loadBatchAutoAnalysisFile(0)`,
    /// `openFileWithAna`): ticket and the dialog it opens.
    preload: Option<(u64, BatchMode)>,
    /// A record already read for the queue item it belongs to: the first record read before the
    /// settings, or 批量精析's cancelled record with its results, kept to continue from.
    ready: Option<(u64, BatchDoc)>,
    run: Option<Run>,
    open_table: u32,
    notice: Option<BatchNotice>,
    notice_id: u32,
    failures: Vec<BatchFailure>,
    /// The end notice waiting for the batch's saves.
    pending_notice: Option<Option<&'static str>>,
    reads: Vec<(u64, PathBuf, bool)>,
    next_read: u64,
    /// Numbers each 批量精析 request round, so a reply of an earlier round is never taken for
    /// a later one on a kept process.
    next_attempt: u64,
    saves: Vec<(u64, String)>,
    jobs: Vec<SaveJob>,
    /// A stopped batch's borrowed engine still being given back.
    releasing: Option<u64>,
}

impl Workbench {
    // -------------------------------------------------------------------- state

    /// A batch runs: its first record being read before the settings (Java's `isBatchAna` is
    /// set before that load), or its record read or analysed.
    pub(super) fn batch_engaged(&self) -> bool {
        self.batch.preload.is_some()
            || self.batch.run.is_some()
            || self.auto_analysis_batch()
            || self.batch.releasing.is_some()
    }

    /// 批量精析 runs or gives its borrowed engine back.
    pub(super) fn batch_flash_engaged(&self) -> bool {
        self.batch
            .run
            .as_ref()
            .is_some_and(|r| matches!(r.work, Work::Flash(_)))
            || self.batch.releasing.is_some()
    }

    /// The foreground engine leaves the open record's position to the batch: 批量分析 between
    /// two records, or 批量精析's analysis process (`prepareForManualFlashAnalysis`).
    pub(super) fn batch_holds_foreground(&self) -> bool {
        self.batch
            .run
            .as_ref()
            .is_some_and(|r| matches!(r.work, Work::Ordinary(_)) || r.holds)
    }

    fn batch_doc(&self) -> Option<&BatchDoc> {
        self.batch.run.as_ref()?.doc.as_ref()
    }

    pub(super) fn batch_doc_id(&self) -> Option<u64> {
        self.batch_doc().map(|d| d.id)
    }

    pub(super) fn batch_record(&self) -> Option<Arc<GameRecord>> {
        self.batch_doc().map(|d| d.record.clone())
    }

    pub(super) fn batch_cursor(&self) -> Option<NodeId> {
        self.batch_doc().map(|d| d.cursor)
    }

    pub(super) fn batch_set_cursor(&mut self, node: NodeId) {
        if let Some(d) = self.batch.run.as_mut().and_then(|r| r.doc.as_mut()) {
            d.cursor = node;
        }
    }

    pub(super) fn batch_change(&self, from: NodeId, to: NodeId) -> Option<MoveChange> {
        let d = self.batch_doc()?;
        self.change_of(&d.record, &d.store, from, to)
    }

    pub(super) fn batch_evaluation(&self, node: NodeId) -> Option<MoveEvaluation> {
        let d = self.batch_doc()?;
        self.evaluation_of(&d.record, &d.store, node)
    }

    /// The position of the batch record the foreground engine analyses.
    pub(super) fn batch_target(&self) -> Option<Target> {
        let d = self.batch_doc()?;
        Some(Target {
            doc: d.id,
            node: d.cursor,
            record: d.record.clone(),
            komi: d.komi,
            rules: d.rules.clone(),
            rules_rev: 0,
        })
    }

    // -------------------------------------------------------------------- queue

    /// A [`BatchCommand`] that needs no chooser; `Err` holds the Java key of a refusal.
    /// `Choose` and `AddFiles` show the adapter's chooser first, then call [`Self::open_batch`] /
    /// [`Self::add_batch_files`].
    pub fn batch_command(&mut self, op: BatchCommand) -> Result<Result<bool, String>, ApiError> {
        let refused = |key: &'static str| key.to_string();
        Ok(match op {
            BatchCommand::Choose { .. } | BatchCommand::AddFiles => {
                return Err(ApiError::new(
                    ErrorKind::InvalidArgument,
                    "the chooser runs in the adapter",
                ));
            }
            BatchCommand::Open { mode, paths, source } => self.open_batch(mode, paths, source).map_err(refused),
            BatchCommand::Setup { mode } => self.setup_batch(mode).map_err(refused),
            BatchCommand::StartOrdinary { settings } => self.start_batch_ordinary(settings)?.map_err(refused),
            BatchCommand::StartFlash {
                start_move,
                end_move,
                visits,
                all_branches,
            } => self.start_batch_flash(start_move, end_move, visits, all_branches)?,
            BatchCommand::CancelSetup => Ok(self.cancel_batch_setup()),
            BatchCommand::CancelFlash => Ok(self.cancel_batch_flash()),
            BatchCommand::Clear => Ok(self.clear_batch()),
            BatchCommand::Move { item, to } => Ok(self.move_batch_item(item, to)),
            BatchCommand::Remove { item } => Ok(self.remove_batch_item(item)),
        })
    }

    /// A batch that could not start from a drop shows its refusal (`AutoAnalyze.conflict`).
    pub fn refuse_batch(&mut self, message_key: &str) {
        self.batch_publish_notice(message_key.to_string(), Vec::new());
    }

    /// A batch still running, or one whose copies are still being written (its end and failures
    /// belong to it until the writes finish).
    fn batch_conflict(&self, mode: BatchMode) -> Option<&'static str> {
        if self.batch.run.is_some()
            || self.batch.releasing.is_some()
            || !self.batch.saves.is_empty()
            || self.batch.pending_notice.is_some()
            || self.auto_analysis_running()
        {
            return Some(CONFLICT);
        }
        if self.whole_game_active() {
            return Some(match mode {
                BatchMode::Ordinary => CONFLICT,
                BatchMode::Flash => WHOLE_GAME_CONFLICT,
            });
        }
        (self.flash_active() || self.flash_releasing()).then_some(CONFLICT)
    }

    fn batch_items(&mut self, paths: Vec<String>, source: OpenSource) -> Vec<Item> {
        paths
            .into_iter()
            .enumerate()
            .map(|(i, path)| {
                self.batch.next_item += 1;
                // Only the first record of a drop is loaded "from temp" (`openOrdinaryBatchAnalysis`).
                Item {
                    id: self.batch.next_item,
                    path,
                    remember: source == OpenSource::Chooser || i > 0,
                }
            })
            .collect()
    }

    /// The chosen or dropped records become the queue (`Batchfiles = files`). The first record
    /// is read before that mode's settings dialog opens, as Java loads it first
    /// (`openOrdinaryBatchAnalysis`, `openFileWithAna`). `Err` holds the Java key of a refusal.
    pub fn open_batch(
        &mut self,
        mode: BatchMode,
        paths: Vec<String>,
        source: OpenSource,
    ) -> Result<bool, &'static str> {
        if paths.is_empty() {
            return Ok(false);
        }
        if let Some(key) = self.batch_conflict(mode) {
            return Err(key);
        }
        let items = self.batch_items(paths, source);
        self.batch.queue = items.into();
        self.batch.setup = None;
        self.batch.ready = None;
        let ticket = self.batch_push_read();
        self.batch.preload = Some((ticket, mode));
        Ok(true)
    }

    /// 进度表's 开始/结束 or 开始/结束(闪电模式) while not running: the dialog for the queue.
    pub fn setup_batch(&mut self, mode: BatchMode) -> Result<bool, &'static str> {
        if self.batch.queue.is_empty() {
            return Ok(false);
        }
        if let Some(key) = self.batch_conflict(mode) {
            return Err(key);
        }
        self.batch.preload = None;
        self.batch.setup = Some(mode);
        Ok(true)
    }

    /// The settings dialog was closed: 批量分析 forgets its records (`endBatchAutoAnalysis`),
    /// 批量精析 keeps them.
    pub fn cancel_batch_setup(&mut self) -> bool {
        match self.batch.setup.take() {
            Some(BatchMode::Ordinary) => {
                self.batch.queue.clear();
                self.batch.ready = None;
                true
            }
            Some(BatchMode::Flash) => true,
            None => false,
        }
    }

    /// 增加棋谱.
    pub fn add_batch_files(&mut self, paths: Vec<String>) -> bool {
        if paths.is_empty() {
            return false;
        }
        let items = self.batch_items(paths, OpenSource::Chooser);
        self.batch.queue.extend(items);
        true
    }

    /// 清空: a running batch stops first (`stopAutoAna(false, true)` saves 批量分析's record).
    pub fn clear_batch(&mut self) -> bool {
        let changed = self.batch.run.is_some() || !self.batch.queue.is_empty() || self.batch.setup.is_some();
        if self.auto_analysis_batch() {
            self.stop_batch_auto_analysis_now();
        } else if !self.batch_stop_loading() && self.batch.run.is_some() {
            self.cancel_batch_flash();
        }
        self.batch.setup = None;
        self.batch.preload = None;
        self.batch.ready = None;
        self.batch.queue.clear();
        self.sync_engine();
        changed
    }

    /// 优先 / 上移 / 下移 of a row (`AnalysisTable`): the current row never moves, nor does a
    /// row move above it.
    pub fn move_batch_item(&mut self, item: u64, to: BatchMove) -> bool {
        let q = &mut self.batch.queue;
        let Some(i) = q.iter().position(|it| it.id == item) else {
            return false;
        };
        match to {
            BatchMove::Prior if i >= 2 => {
                let it = q.remove(i).unwrap();
                q.insert(1, it);
            }
            BatchMove::Up if i >= 2 => q.swap(i, i - 1),
            BatchMove::Down if i >= 1 && i + 1 < q.len() => q.swap(i, i + 1),
            _ => return false,
        }
        true
    }

    /// 删除 of a row other than the current one.
    pub fn remove_batch_item(&mut self, item: u64) -> bool {
        match self.batch.queue.iter().position(|it| it.id == item) {
            Some(i) if i >= 1 => {
                self.batch.queue.remove(i);
                true
            }
            _ => false,
        }
    }

    // -------------------------------------------------------------------- starting

    /// The 批量分析 dialog's 开始分析 (`StartAnaDialog.apply`, `startAutoAna(batch)`). The
    /// records are always saved; the dialog's fields are saved as the automatic-analysis settings.
    pub fn start_batch_ordinary(
        &mut self,
        settings: AutoAnalysisSettings,
    ) -> Result<Result<bool, &'static str>, ApiError> {
        if self.batch.setup != Some(BatchMode::Ordinary) || self.batch.queue.is_empty() {
            return Ok(Ok(false));
        }
        if let Err(key) = check_auto_settings(&settings) {
            return Ok(Err(key));
        }
        if let Some(key) = self.batch_conflict(BatchMode::Ordinary) {
            return Ok(Err(key));
        }
        if self.auto_engine().is_none() {
            return Ok(Err("AutoAnalyze.engineUnavailable"));
        }
        // The batch dialog has no 自动保存棋谱 box: Java checks the shared box for the batch
        // (`chkAnaAutoSave.setSelected(true)`), and it stays checked afterwards.
        let saved = AutoAnalysisSettings {
            auto_save: true,
            ..settings.clone()
        };
        if let Some(key) = self.save_auto_analysis_settings(saved)? {
            return Ok(Err(key));
        }
        // Java opens 进度表 when a batch of several records starts (`BatchAnaNum == 0`).
        if self.batch.queue.len() > 1 {
            self.batch.open_table = self.batch.open_table.wrapping_add(1);
        }
        self.batch_begin(Work::Ordinary(settings), false, false);
        Ok(Ok(true))
    }

    /// The 批量精析 dialog's 开始分析 (`flashAnalyzeGameBatch`). The move range is the
    /// automatic-analysis fields the dialog shares; 每手总计算量 is `batch-analysis-playouts`.
    pub fn start_batch_flash(
        &mut self,
        start_move: i32,
        end_move: i32,
        visits: u32,
        all_branches: bool,
    ) -> Result<Result<bool, String>, ApiError> {
        if self.batch.setup != Some(BatchMode::Flash) || self.batch.queue.is_empty() {
            return Ok(Ok(false));
        }
        if let Some(key) = self.batch_conflict(BatchMode::Flash) {
            return Ok(Err(key.to_string()));
        }
        let borrow = self.config.analysis_engine.reuse_current_engine;
        if borrow {
            let availability = self.engines.lease_availability();
            if availability != crate::engine::runtime::LeaseAvailability::Available {
                return Ok(Err(reuse_status_key(availability)));
            }
        } else if self.analysis_command_base(false).trim().is_empty() {
            return Ok(Err(ANALYSIS_COMMAND_MISSING.to_string()));
        }
        self.commit_config(|c| {
            c.analysis_engine.batch_visits = visits;
            c.auto_analysis.start_move = start_move;
            c.auto_analysis.end_move = end_move;
        })?;
        let params = FlashParams {
            start_move,
            end_move,
            visits,
            all_branches,
        };
        self.batch_begin(Work::Flash(params), borrow, !borrow && !self.paused);
        Ok(Ok(true))
    }

    fn batch_begin(&mut self, work: Work, borrow: bool, holds: bool) {
        self.batch.setup = None;
        self.batch.preload = None;
        self.batch.failures.clear();
        self.auto_analysis_cancel_pending();
        // `MANUAL_FLASH` / manual auto analysis: an automatic quick analysis hands its engine over.
        self.quick_hand_over();
        let item = self.batch.queue.front().expect("a started batch has a record").id;
        // The record read before the settings, or kept by a cancelled 批量精析 with its results,
        // is analysed as it is.
        let ready = self
            .batch
            .ready
            .take()
            .filter(|(id, _)| *id == item)
            .map(|(_, doc)| doc);
        self.batch.run = Some(Run {
            work,
            borrow,
            item,
            read: None,
            doc: None,
            engine: None,
            holds,
            done: 0,
            total: 0,
        });
        match ready {
            Some(doc) => self.batch_analyse(doc),
            None => self.batch_read_current(),
        }
        self.sync_engine();
    }

    // -------------------------------------------------------------------- reading

    /// Asks the adapter to read the first row's file; returns the read's ticket.
    fn batch_push_read(&mut self) -> u64 {
        let path = PathBuf::from(&self.batch.queue.front().expect("a queued record").path);
        self.batch.next_read += 1;
        let ticket = self.batch.next_read;
        self.batch.reads.push((ticket, path, self.config.edit.no_capture));
        ticket
    }

    fn batch_read_current(&mut self) {
        let Some(id) = self.batch.queue.front().map(|item| item.id) else {
            return;
        };
        let ticket = self.batch_push_read();
        if let Some(run) = self.batch.run.as_mut() {
            run.item = id;
            run.read = Some(ticket);
            run.doc = None;
            run.done = 0;
            run.total = 0;
        }
    }

    /// Reads for the adapter to run off the lock: ticket, file and the `ui.no-capture` rule.
    pub fn take_batch_reads(&mut self) -> Vec<(u64, PathBuf, bool)> {
        std::mem::take(&mut self.batch.reads)
    }

    /// Hands back a queued record's read. Returns `false` for a read the batch no longer wants.
    pub fn complete_batch_read(&mut self, ticket: u64, result: Result<(RecordFormat, GameRecord), ReadError>) -> bool {
        if let Some((preload, mode)) = self.batch.preload {
            if preload == ticket {
                self.batch.preload = None;
                self.batch_preloaded(mode, result);
                return true;
            }
        }
        if !self.batch.run.as_ref().is_some_and(|r| r.read == Some(ticket)) {
            return false;
        }
        self.batch.run.as_mut().unwrap().read = None;
        match result {
            Ok((format, record)) => {
                let doc = self.batch_make_doc(format, record);
                self.batch_analyse(doc);
            }
            Err(e) => {
                self.batch_unreadable(&e);
                self.batch_read_failed();
            }
        }
        true
    }

    /// The first record, read before the settings dialog. An unreadable one ends 批量分析
    /// without the dialog (`loadBatchAutoAnalysisFile` → `endBatchAutoAnalysis`); 批量精析's
    /// dialog opens anyway (`openFileWithAna` ignores the failed load) and the record is read
    /// again, and skipped, when the batch starts.
    fn batch_preloaded(&mut self, mode: BatchMode, result: Result<(RecordFormat, GameRecord), ReadError>) {
        let Some(item) = self.batch.queue.front().map(|item| item.id) else {
            return;
        };
        match result {
            Ok((format, record)) => {
                let doc = self.batch_make_doc(format, record);
                self.batch.ready = Some((item, doc));
                self.batch_show_setup(mode);
            }
            Err(e) if mode == BatchMode::Ordinary => {
                self.batch_unreadable(&e);
                self.batch_end(Some(FAILED_TO_OPEN_FILE), true);
            }
            Err(_) => self.batch_show_setup(mode),
        }
    }

    /// The settings dialog for the opened records. Java checks the shared 自动保存棋谱 box
    /// before showing either batch dialog (`chkAnaAutoSave.setSelected(true)`), and it stays
    /// checked even if the dialog is cancelled.
    fn batch_show_setup(&mut self, mode: BatchMode) {
        self.batch.setup = Some(mode);
        if !self.config.auto_analysis.auto_save {
            if let Err(e) = self.commit_config(|c| c.auto_analysis.auto_save = true) {
                self.log(true, format!("batch: auto-save setting not written: {}", e.message));
            }
        }
    }

    fn batch_unreadable(&mut self, e: &ReadError) {
        let path = self
            .batch
            .queue
            .front()
            .expect("the read record is the first row")
            .path
            .clone();
        self.batch.failures.push(BatchFailure {
            path,
            message_key: FAILED_TO_OPEN_FILE.into(),
            detail: e.to_string(),
        });
    }

    /// The first row's record as a background document of the batch, opened like an open: komi
    /// rule, branch numbering, its saved analysis, the recent list unless it came from a drop.
    fn batch_make_doc(&mut self, format: RecordFormat, mut record: GameRecord) -> BatchDoc {
        let (path, remember) = {
            let item = self.batch.queue.front().expect("the read record is the first row");
            (item.path.clone(), item.remember)
        };
        // The open's komi rule (`resetAllNoKomi`), against the session komi.
        let use_record_komi = record.komi_declared && (self.config.record.read_komi || record.has_setup_or_handicap);
        let komi = if use_record_komi { record.info.komi } else { self.komi };
        record.info.komi = komi;
        record.number_branches_as_read(self.config.display.branch_numbers_from_one);
        let rules = match format {
            RecordFormat::Sgf => record
                .info
                .rules
                .as_deref()
                .and_then(KataRules::parse)
                .filter(KataRules::has_required_fields),
            RecordFormat::Gib => None,
        };
        let id = self.next_doc;
        self.next_doc += 1;
        let mut store = AnalysisStore::default();
        store.reset(id);
        store.load(
            &record,
            saved_analysis::PRIMARY,
            &self.config.engine.analysis,
            self.config.display.no_sugg_circle,
        );
        if remember {
            self.config.remember_recent(&path);
            if let Some(parent) = Path::new(&path).parent() {
                self.persisted.last_folder = Some(parent.to_string_lossy().into_owned());
            }
            let _ = self.save_config();
            let _ = self.save_persisted();
        }
        BatchDoc {
            id,
            path,
            record: Arc::new(record),
            store,
            komi,
            rules,
            cursor: ROOT,
        }
    }

    /// The run analyses `doc`, the first row's record.
    fn batch_analyse(&mut self, doc: BatchDoc) {
        let (id, komi) = (doc.id, doc.komi);
        let run = self.batch.run.as_mut().expect("a running batch");
        run.doc = Some(doc);
        match &run.work {
            Work::Ordinary(settings) => {
                let settings = settings.clone();
                if let Err(key) = self.start_batch_auto_analysis(id, komi, settings) {
                    self.batch.run = None;
                    self.batch_end(Some(key), false);
                    self.sync_engine();
                }
            }
            Work::Flash(_) => self.batch_flash_record(),
        }
    }

    /// A queued record that could not be read. 批量分析 ends and forgets its queue
    /// (`loadBatchAutoAnalysisFile` → `endBatchAutoAnalysis`), naming the file. 批量精析 skips it
    /// and goes on with the next record: Java's `loadAutoBatchFile` ignores the failed load, so
    /// the previous record is analysed again and saved under the unreadable file's name.
    fn batch_read_failed(&mut self) {
        match self.batch.run.as_ref().map(|r| matches!(r.work, Work::Ordinary(_))) {
            Some(true) => {
                self.batch.run = None;
                self.batch_end(Some(FAILED_TO_OPEN_FILE), true);
                self.sync_engine();
            }
            Some(false) => self.batch_flash_advance(),
            None => {}
        }
    }

    /// 停止自动(批量)分析 / 进度表's 开始/结束 while 批量分析's record is still being read: the
    /// batch ends there and nothing more is read or analysed (`stopAutoAna` cancelling a pending
    /// start). Before its settings, the cancelled start ends the batch without a message
    /// (`ManualAutoAnalysisStartFailure.CANCELLED`).
    pub(super) fn batch_stop_loading(&mut self) -> bool {
        if matches!(self.batch.preload, Some((_, BatchMode::Ordinary))) {
            self.batch.preload = None;
            self.batch.queue.clear();
            self.batch.ready = None;
            return true;
        }
        if self.auto_analysis_batch()
            || !self
                .batch
                .run
                .as_ref()
                .is_some_and(|r| matches!(r.work, Work::Ordinary(_)))
        {
            return false;
        }
        self.batch.run = None;
        self.paused = true;
        self.batch_end(Some(STOPPED), true);
        self.sync_engine();
        true
    }

    /// Exit: the batch ends with the application, so a read still in flight starts nothing.
    pub(super) fn batch_shutdown(&mut self) {
        self.batch.run = None;
        self.batch.preload = None;
        self.batch.ready = None;
        self.batch.reads.clear();
    }

    // -------------------------------------------------------------------- 批量分析

    /// The foreground engine's line for 批量分析's record `doc`: stored in that record's store
    /// (automatic analysis keeps the accepted packet, `protectedCache` off) and checked against
    /// the run's conditions.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn batch_foreground_result(
        &mut self,
        doc: u64,
        node: NodeId,
        stamp: u64,
        komi: f64,
        to_play: Stone,
        engine_name: String,
        line: InfoLine,
        elapsed: Duration,
    ) {
        let limit = self.config.engine.analysis.limit_branch_length;
        let capture = !self.config.edit.no_capture;
        let Some(d) = self
            .batch
            .run
            .as_mut()
            .and_then(|r| r.doc.as_mut())
            .filter(|d| d.id == doc)
        else {
            return;
        };
        if komi != d.komi || !d.record.contains(node) || d.record.node(node).stamp != stamp {
            return;
        }
        let Some((mut result, ownership)) = node_analysis(node, to_play, komi, engine_name, line) else {
            return;
        };
        let visits = result.visits;
        let first = result.candidates.first().map_or(0, |c| c.visits);
        fill_pv_captures(&mut result, &d.record.position(node), limit, capture);
        d.store.insert(node, stamp, result, ownership);
        if node == d.cursor && !self.paused {
            self.auto_analysis_result(node, visits, first, elapsed);
        }
    }

    /// 批量分析's record is done (`autoAnaSaveAndLoad`): `summary` goes into its root comment, the
    /// record is saved, then the next one loads after a natural end; a forced stop or the last
    /// record ends the batch and forgets the queue.
    pub(super) fn batch_auto_finished(&mut self, completed: bool, summary: Option<String>) {
        if let (Some(comment), Some(d)) = (summary, self.batch.run.as_mut().and_then(|r| r.doc.as_mut())) {
            let _ = Arc::make_mut(&mut d.record).apply(&DocumentEdit::SetComment { node: ROOT, comment });
        }
        self.batch_save_current();
        if completed && self.batch.queue.len() > 1 {
            self.batch.queue.pop_front();
            self.batch_read_current();
            self.sync_engine();
            return;
        }
        // Java stops pondering and leaves the engine idle.
        self.batch.run = None;
        self.paused = true;
        self.batch_end(Some(if completed { COMPLETE } else { STOPPED }), true);
        self.sync_engine();
    }

    /// 批量分析's run ended without its stop path (engine switched or closed, space with
    /// 手动暂停分析则退出自动分析): the current record is not saved and the queue stays.
    pub(super) fn batch_auto_ended(&mut self, key: Option<&'static str>) {
        if !self
            .batch
            .run
            .as_ref()
            .is_some_and(|r| matches!(r.work, Work::Ordinary(_)))
        {
            return;
        }
        self.batch.run = None;
        self.batch_end(key, false);
        self.sync_engine();
    }

    // -------------------------------------------------------------------- 批量精析

    fn batch_flash_params(&self) -> Option<FlashParams> {
        match self.batch.run.as_ref()?.work {
            Work::Flash(p) => Some(p),
            Work::Ordinary(_) => None,
        }
    }

    /// Asks about the current record's chosen moves (`flashAnalyzeGame(false, allBranches)` with
    /// the batch's range and visits); a record with nothing to ask is saved at once
    /// (`finishSuccessfulEmptyRequest`).
    fn batch_flash_record(&mut self) {
        let Some(p) = self.batch_flash_params() else { return };
        let visits = p.visits.max(2);
        let target = if p.all_branches {
            FlashTarget::AllBranches
        } else {
            FlashTarget::MoveRange {
                start_move: p.start_move,
                end_move: p.end_move,
            }
        };
        let (nodes, rec, komi, rules) = {
            let d = self.batch_doc().expect("a read batch record");
            let nodes = flash_nodes_of(
                &d.record,
                &d.store,
                target,
                u64::from(visits),
                self.config.analysis_engine.always_override,
            );
            (nodes, d.record.clone(), d.komi, self.batch_query_rules())
        };
        {
            let run = self.batch.run.as_mut().unwrap();
            run.done = 0;
            run.total = nodes.len() as u32;
        }
        if nodes.is_empty() {
            return self.batch_flash_record_done();
        }
        if self.batch.run.as_ref().unwrap().borrow {
            match self.batch.run.as_mut().unwrap().engine.as_mut() {
                Some(FlashEngine::Lease { queue, ready, .. }) => {
                    *queue = nodes.into();
                    if *ready {
                        self.batch_lease_next();
                    }
                }
                _ => match self.engines.begin_lease() {
                    Ok(lease) => {
                        self.batch.run.as_mut().unwrap().engine = Some(FlashEngine::Lease {
                            lease,
                            ready: false,
                            queue: nodes.into(),
                            ending: None,
                        });
                    }
                    Err(availability) => {
                        let key = reuse_status_key(availability);
                        self.batch_flash_failed_owned(key);
                        return;
                    }
                },
            }
        } else {
            let command = self.tuned_command_for(&self.analysis_command_base(false), visits);
            let instance = match self.batch.run.as_mut().unwrap().engine.take() {
                Some(FlashEngine::Process {
                    instance, command: c, ..
                }) if c == command && self.engines.worker_alive(instance) => Ok(instance),
                Some(FlashEngine::Process { instance, .. }) => {
                    self.engines.terminate(instance);
                    self.take_analysis_process(&command)
                }
                _ => self.take_analysis_process(&command),
            };
            let instance = match instance {
                Ok(instance) => instance,
                Err(message) => return self.batch_flash_engine_failed(&command, true, message, Vec::new()),
            };
            self.batch.next_attempt += 1;
            let prefix = format!("batch{}", self.batch.next_attempt);
            let (queries, sent) = self.send_record_queries(instance, &rec, komi, 0, visits, &nodes, &prefix, rules);
            if !sent {
                self.engines.terminate(instance);
                return self.batch_flash_engine_failed(&command, true, "input closed".into(), Vec::new());
            }
            self.batch.run.as_mut().unwrap().engine = Some(FlashEngine::Process {
                instance,
                command,
                queries,
            });
        }
        self.drain_engine_events();
        self.sync_engine();
    }

    /// The rules an analysis process searches a batch record with (`resolveLocalAnalysisRules`):
    /// under 使用当前规则 the record's own declared rules, as Java's engine takes them on when it
    /// loads the record; otherwise as for any flash analysis.
    fn batch_query_rules(&self) -> serde_json::Value {
        if self.config.analysis_engine.use_current_rules {
            if let Some(rules) = self.batch_doc().and_then(|d| d.rules.as_ref()) {
                if let Ok(value) = serde_json::from_str(&rules.to_gtp_argument()) {
                    return value;
                }
            }
        }
        self.quick_rules()
    }

    /// The rules a borrowed engine searches a batch record with (`resolveRemoteGtpRules`):
    /// 使用指定规则, else the record's declared rules.
    fn batch_lease_rules(&self) -> Option<String> {
        let a = &self.config.analysis_engine;
        if a.use_current_rules {
            return self.batch_doc()?.rules.as_ref().map(KataRules::to_gtp_argument);
        }
        let rules = a.specific_rules.trim();
        Some(if rules.is_empty() { "tromp-taylor" } else { rules }.to_string())
    }

    /// Asks the borrowed engine about the record's next move, or saves the record.
    fn batch_lease_next(&mut self) {
        let visits = self.batch_flash_params().map_or(2, |p| p.visits.max(2));
        loop {
            let rules = self.batch_lease_rules();
            let Some(run) = self.batch.run.as_mut() else { return };
            let Some(FlashEngine::Lease {
                lease,
                queue,
                ending: None,
                ..
            }) = run.engine.as_mut()
            else {
                return;
            };
            let lease = *lease;
            let Some(node) = queue.pop_front() else {
                return self.batch_flash_record_done();
            };
            let d = run.doc.as_ref().expect("a read batch record");
            if !d.record.contains(node) {
                run.done += 1;
                continue;
            }
            let target = Target {
                doc: d.id,
                node,
                record: d.record.clone(),
                komi: d.komi,
                rules: d.rules.clone(),
                rules_rev: 0,
            };
            if !self
                .engines
                .lease_search(lease, &target, u64::from(visits), rules.as_deref())
            {
                self.batch_flash_failed(REQUEST_DISPATCH_FAILED);
            }
            return;
        }
    }

    fn batch_absorb(&mut self, q: &Query, line: InfoLine, borrowed: bool) {
        let Some(d) = self.batch.run.as_mut().and_then(|r| r.doc.as_mut()) else {
            return;
        };
        let mut store = std::mem::take(&mut d.store);
        let rec = d.record.clone();
        self.absorb_background_result(&mut store, &rec, q, line, borrowed);
        if let Some(d) = self.batch.run.as_mut().and_then(|r| r.doc.as_mut()) {
            d.store = store;
        }
    }

    /// Every chosen move of the record was answered: it is saved (`flashAutoAnaSaveAndLoad`) and
    /// the batch goes on.
    fn batch_flash_record_done(&mut self) {
        self.batch_save_current();
        self.batch_flash_advance();
    }

    /// The next record loads; after the last one the batch is complete and its engine ends with
    /// 结束分析后退出引擎 (an analysis process otherwise stays for the next analysis).
    fn batch_flash_advance(&mut self) {
        self.batch.queue.pop_front();
        if !self.batch.queue.is_empty() {
            self.batch_read_current();
            return;
        }
        let run = self.batch.run.as_mut().unwrap();
        match run.engine.take() {
            Some(FlashEngine::Lease {
                lease, ready, queue, ..
            }) => {
                run.engine = Some(FlashEngine::Lease {
                    lease,
                    ready,
                    queue,
                    ending: Some(Some(COMPLETE)),
                });
                self.engines.end_lease(lease);
                self.drain_engine_events();
                return;
            }
            Some(FlashEngine::Process { instance, command, .. }) => {
                if self.config.analysis_engine.auto_quit {
                    self.engines.terminate(instance);
                } else {
                    self.keep_analysis_process(instance, command);
                }
            }
            None => {}
        }
        self.batch.run = None;
        self.batch_end(Some(COMPLETE), true);
        self.sync_engine();
    }

    /// 取消分析 / 开始/结束(闪电模式) (`destroyAnalysisEngine`): the engine ends, the current
    /// record is not saved and stays the first row; the foreground engine stays stopped.
    pub fn cancel_batch_flash(&mut self) -> bool {
        if self.batch_flash_params().is_none() {
            return false;
        }
        self.batch_stop_flash_engine(true);
        // The record and the results it has stay for 开始/结束(闪电模式) to continue from
        // (`verifyCurrentKifu` keeps the board when it is still the first row's record).
        if let Some(Run {
            item, doc: Some(doc), ..
        }) = self.batch.run.take()
        {
            self.batch.ready = Some((item, doc));
        }
        self.batch_end(None, false);
        self.sync_engine();
        true
    }

    /// Ends 批量精析's engine: an analysis process ends, a borrowed engine is given back (the
    /// batch no longer waits for it). `stop_foreground` leaves the foreground engine stopped when
    /// the process held its position (`destroyAnalysisEngine`).
    fn batch_stop_flash_engine(&mut self, stop_foreground: bool) {
        let Some(run) = self.batch.run.as_mut() else { return };
        if stop_foreground && run.holds {
            self.paused = true;
        }
        match run.engine.take() {
            Some(FlashEngine::Process { instance, .. }) => self.engines.terminate(instance),
            Some(FlashEngine::Lease { lease, ending, .. }) => {
                if ending.is_none() {
                    self.engines.end_lease(lease);
                }
                if self.engines.lease_alive(lease) {
                    self.batch.releasing = Some(lease);
                }
            }
            None => {}
        }
        self.drain_engine_events();
    }

    /// A refused request or a lost borrowed search: the batch stops at the current record, the
    /// queue stays and an analysis process stays for reuse.
    fn batch_flash_failed(&mut self, key: &'static str) {
        let Some(run) = self.batch.run.as_mut() else { return };
        match run.engine.take() {
            Some(FlashEngine::Process { instance, command, .. }) => self.keep_analysis_process(instance, command),
            Some(FlashEngine::Lease { lease, ending, .. }) => {
                // The batch ends once the engine is confirmed back (`LeaseEnded`).
                run.engine = Some(FlashEngine::Lease {
                    lease,
                    ready: false,
                    queue: VecDeque::new(),
                    ending: Some(Some(key)),
                });
                if ending.is_none() {
                    self.engines.end_lease(lease);
                }
                self.drain_engine_events();
                return;
            }
            None => {}
        }
        self.batch.run = None;
        self.batch_end(Some(key), false);
        self.sync_engine();
    }

    fn batch_flash_failed_owned(&mut self, key: String) {
        // The availability keys are Java resource keys; the notice carries them as text.
        self.batch.run = None;
        let failures = std::mem::take(&mut self.batch.failures);
        self.batch_publish_notice(key, failures);
        self.sync_engine();
    }

    /// The analysis process failed (`showErrMsg`): reported like an engine failure; the batch
    /// stops at the current record, which stays the first row.
    fn batch_flash_engine_failed(&mut self, command: &str, during_startup: bool, message: String, output: Vec<String>) {
        if let Some(run) = self.batch.run.as_mut() {
            if run.holds {
                self.paused = true;
            }
            run.engine = None;
        }
        self.engine_failed(EngineFailure {
            instance: 0,
            profile_id: String::new(),
            engine_name: "AnalysisEngine".into(),
            command: command.to_string(),
            during_startup,
            message,
            output,
        });
        self.batch.run = None;
        self.batch_end(None, false);
        self.sync_engine();
    }

    /// Space / 继续分析 while 批量精析's analysis process holds the position: the foreground
    /// engine analyses the open record again.
    pub(super) fn batch_toggle_pause(&mut self) -> bool {
        if self.paused {
            return false;
        }
        match self.batch.run.as_mut() {
            Some(run) if run.holds => {
                run.holds = false;
                true
            }
            _ => false,
        }
    }

    /// The foreground engine was switched, restarted or closed: 批量精析 borrowing it stops at
    /// the current record; an analysis process continues and no longer holds the position.
    pub(super) fn batch_engine_changed(&mut self) {
        let Some(run) = self.batch.run.as_mut() else { return };
        if matches!(run.engine, Some(FlashEngine::Lease { .. })) {
            self.batch_stop_flash_engine(false);
            self.batch.run = None;
            self.batch_end(None, false);
        } else {
            run.holds = false;
        }
    }

    /// Runtime events of 批量精析; any other event is handed back.
    pub(super) fn batch_event(&mut self, ev: RuntimeEvent) -> Option<RuntimeEvent> {
        let (lease, process) = match self.batch.run.as_ref().and_then(|r| r.engine.as_ref()) {
            Some(FlashEngine::Lease { lease, .. }) => (Some(*lease), None),
            Some(FlashEngine::Process { instance, .. }) => (None, Some(*instance)),
            None => (None, None),
        };
        let releasing = self.batch.releasing;
        match ev {
            RuntimeEvent::LeaseReady { lease: l } if lease == Some(l) => {
                if let Some(FlashEngine::Lease { ready, .. }) = self.batch.run.as_mut().and_then(|r| r.engine.as_mut())
                {
                    *ready = true;
                }
                self.batch_lease_next();
            }
            RuntimeEvent::LeaseResult {
                lease: l,
                node,
                stamp,
                komi,
                rules_rev,
                to_play,
                line,
                ..
            } if lease == Some(l) => {
                let searching = matches!(
                    self.batch.run.as_ref().and_then(|r| r.engine.as_ref()),
                    Some(FlashEngine::Lease { ending: None, .. })
                );
                if searching {
                    self.batch.run.as_mut().unwrap().done += 1;
                    self.batch_absorb(
                        &Query {
                            node,
                            stamp,
                            komi,
                            rules_rev,
                            to_play,
                        },
                        line,
                        true,
                    );
                    self.batch_lease_next();
                }
            }
            RuntimeEvent::LeaseSearchFailed { lease: l } if lease == Some(l) => {
                self.batch_flash_failed(REQUEST_DISPATCH_FAILED)
            }
            RuntimeEvent::LeaseEnded { lease: l, failure } if lease == Some(l) || releasing == Some(l) => {
                if let Some(f) = failure {
                    self.log(true, format!("batch foreground engine lease {l} ended: {f:?}"));
                }
                let restore_failed = failure.is_some_and(|f| f.unrestores() && f != LeaseFailure::TransportClosed);
                if releasing == Some(l) {
                    self.batch.releasing = None;
                    if restore_failed {
                        self.batch_publish_notice(FOREGROUND_RESTORE_FAILED.into(), Vec::new());
                    }
                    self.borrowed_engine_returned();
                } else {
                    let ending = match self.batch.run.as_ref().and_then(|r| r.engine.as_ref()) {
                        Some(FlashEngine::Lease { ending, .. }) => *ending,
                        _ => None,
                    };
                    self.batch.run = None;
                    let key = if restore_failed {
                        Some(FOREGROUND_RESTORE_FAILED)
                    } else {
                        // An end the batch did not ask for is a lost search.
                        ending.unwrap_or(Some(REQUEST_DISPATCH_FAILED))
                    };
                    let complete = key == Some(COMPLETE);
                    self.batch_end(key, complete);
                }
                self.sync_engine();
            }
            RuntimeEvent::WorkerLine { instance, line } if process == Some(instance) => self.batch_worker_line(&line),
            RuntimeEvent::WorkerExited {
                instance,
                message,
                output,
            } if process == Some(instance) => {
                let command = match self.batch.run.as_ref().and_then(|r| r.engine.as_ref()) {
                    Some(FlashEngine::Process { command, .. }) => command.clone(),
                    _ => String::new(),
                };
                self.batch_flash_engine_failed(&command, false, message, output);
            }
            RuntimeEvent::WorkerLaunchFailed { instance, message } if process == Some(instance) => {
                let command = match self.batch.run.as_ref().and_then(|r| r.engine.as_ref()) {
                    Some(FlashEngine::Process { command, .. }) => command.clone(),
                    _ => String::new(),
                };
                self.engines.terminate(instance);
                self.batch_flash_engine_failed(&command, true, message, Vec::new());
            }
            other => return Some(other),
        }
        None
    }

    fn batch_worker_line(&mut self, line: &str) {
        let Some(size) = self.batch_doc().map(|d| d.record.size) else {
            return;
        };
        let Some(Run {
            engine: Some(FlashEngine::Process { queries, .. }),
            ..
        }) = self.batch.run.as_mut()
        else {
            return;
        };
        let (query, line) = match kata_analysis::parse(line, size) {
            Response::Result { id, line } => match queries.remove(&id) {
                Some(q) => (q, line),
                None => return,
            },
            Response::Error { id, message } => {
                let ours = id.is_some_and(|id| queries.contains_key(&id));
                self.log(true, format!("batch analysis error: {message}"));
                if ours {
                    self.batch_flash_failed(REQUEST_DISPATCH_FAILED);
                    self.sync_engine();
                }
                return;
            }
            Response::Other => return,
        };
        self.batch.run.as_mut().unwrap().done += 1;
        self.batch_absorb(&query, line, false);
        let finished = matches!(
            self.batch.run.as_ref().and_then(|r| r.engine.as_ref()),
            Some(FlashEngine::Process { queries, .. }) if queries.is_empty()
        );
        if finished {
            self.batch_flash_record_done();
        }
    }

    // -------------------------------------------------------------------- saving and ending

    /// Saves the current record beside its source (`<name>_<已分析>_<timestamp>.sgf`, always
    /// SGF) with its analysis, through the serial writer.
    fn batch_save_current(&mut self) {
        let Some(mut d) = self.batch.run.as_mut().and_then(|r| r.doc.take()) else {
            return;
        };
        let edits = self.save_comment_edits(&d.record, &d.store, None);
        if !edits.is_empty() {
            let rec = Arc::make_mut(&mut d.record);
            for edit in &edits {
                let _ = rec.apply(edit);
            }
        }
        let stamp = chrono::Local::now().format("%Y%m%d%H%M%S").to_string();
        let label = super::exporting::java_text(self.active_language, "Leelaz.analyzed");
        let work = self.storage.save_dir.parent().unwrap_or(Path::new(".")).to_path_buf();
        let path = analyzed_output(Path::new(&d.path), &work, &stamp, &label);
        let path_text = path.to_string_lossy().into_owned();
        let text = self.analyzed_text(&d.record, &d.store, None, None, APPLICATION);
        let ticket = self.next_save;
        self.next_save += 1;
        self.saves.push(PendingSave {
            ticket,
            document: d.id,
            revision: 0,
            path: path_text.clone(),
            export: None,
        });
        self.batch.saves.push((ticket, path_text));
        self.batch.jobs.push(SaveJob {
            ticket,
            path,
            create_parent: false,
            text,
        });
    }

    pub(super) fn take_batch_saves(&mut self) -> Vec<SaveJob> {
        std::mem::take(&mut self.batch.jobs)
    }

    /// A batch save finished: a failure is kept for the batch's notice. Returns `false` for a
    /// save that is not the batch's.
    pub(super) fn batch_save_finished(&mut self, ticket: u64, result: &Result<(), String>) -> bool {
        let Some(at) = self.batch.saves.iter().position(|(t, _)| *t == ticket) else {
            return false;
        };
        let (_, path) = self.batch.saves.remove(at);
        if let Err(detail) = result {
            self.batch.failures.push(BatchFailure {
                path,
                message_key: SAVE_FILE_FAILED.into(),
                detail: detail.clone(),
            });
        }
        if self.batch.saves.is_empty() {
            if let Some(key) = self.batch.pending_notice.take() {
                self.batch_notify(key);
            }
        }
        true
    }

    /// The batch ended: `key` explains it once its saves are written (with every failure of the
    /// batch); `clear` forgets the queue (`endBatchAutoAnalysis`).
    fn batch_end(&mut self, key: Option<&'static str>, clear: bool) {
        if clear {
            self.batch.queue.clear();
            self.batch.ready = None;
        }
        if self.batch.saves.is_empty() {
            self.batch_notify(key);
        } else {
            self.batch.pending_notice = Some(key);
        }
    }

    fn batch_notify(&mut self, key: Option<&'static str>) {
        let key = match key {
            Some(key) => key.to_string(),
            // Without an explanation of its own the end is shown only for its failures.
            None => match self.batch.failures.first() {
                Some(f) => f.message_key.clone(),
                None => return,
            },
        };
        let failures = std::mem::take(&mut self.batch.failures);
        self.batch_publish_notice(key, failures);
    }

    fn batch_publish_notice(&mut self, message_key: String, failures: Vec<BatchFailure>) {
        self.batch.notice_id = self.batch.notice_id.wrapping_add(1);
        self.batch.notice = Some(BatchNotice {
            id: self.batch.notice_id,
            message_key,
            failures,
        });
    }

    // -------------------------------------------------------------------- projection

    pub(super) fn batch_view(&self) -> BatchView {
        let running = self.batch.run.as_ref().map(|r| {
            let (mode, borrowed) = match r.work {
                Work::Ordinary(_) => (BatchMode::Ordinary, false),
                Work::Flash(_) => (BatchMode::Flash, r.borrow),
            };
            let (done, total, loading) = match mode {
                BatchMode::Ordinary => match self.auto_analysis_progress().filter(|_| self.auto_analysis_batch()) {
                    Some((done, total)) => (done, total, false),
                    None => (0, 0, true),
                },
                BatchMode::Flash => {
                    let waiting = r.doc.is_none() || matches!(r.engine, Some(FlashEngine::Lease { ready: false, .. }));
                    (r.done, r.total, waiting)
                }
            };
            BatchRunView {
                mode,
                item: r.item,
                loading,
                done,
                total,
                borrowed,
            }
        });
        BatchView {
            items: self
                .batch
                .queue
                .iter()
                .map(|it| BatchItemView {
                    id: it.id,
                    name: Path::new(&it.path)
                        .file_name()
                        .map_or_else(|| it.path.clone(), |n| n.to_string_lossy().into_owned()),
                    path: it.path.clone(),
                })
                .collect(),
            setup: self.batch.setup,
            loading: self.batch.preload.map(|(_, mode)| mode),
            running,
            open_table: self.batch.open_table,
            notice: self.batch.notice.clone(),
        }
    }
}

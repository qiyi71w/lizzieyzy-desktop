//! 批量分析 (Java `LizzieFrame.openFileWithAna`, `AnalysisTable`, `StartAnaDialog` for several
//! records): a queue of record files analysed one after another, each saved beside its source as
//! `<name>_<已分析>_<yyyyMMddHHmmss>.sgf`.
//!
//! The queued records are background documents of their own: the open record is never replaced,
//! so the user browses and edits it while the queue runs. 批量分析 is the automatic analysis of
//! each record with the foreground engine (its stop, pause and strengthening questions are
//! [`super::Command::StopAutoAnalysis`], the pause toggle and
//! [`super::Command::AnswerAutoAnalysisDiff`]); 批量精析 is a flash analysis of each record with
//! the 精析设置 engine.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::OpenSource;
use crate::settings::AutoAnalysisSettings;

/// Which batch: 批量分析 (automatic analysis of each record) or 批量精析 (flash analysis).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub enum BatchMode {
    Ordinary,
    Flash,
}

/// A 进度表 row's 优先 / 上移 / 下移.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub enum BatchMove {
    /// Right after the current record (`BatchAnaNum + 1`).
    Prior,
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum BatchCommand {
    /// 批量分析 (Ctrl+O) / 批量精析: the multi-file chooser (`LizzieFrame.chooseKifu`, the last
    /// folder), then that mode's settings dialog for the chosen records. A conflicting analysis
    /// answers `Refused` with `AutoAnalyze.conflict`.
    Choose { mode: BatchMode },
    /// Known files (several files dropped on the board start 批量分析, as Java's
    /// `openOrdinaryBatchAnalysis(files, true)`; automation): like `Choose` after the chooser.
    /// Records opened from a drop do not enter the recent list or the last folder.
    Open {
        mode: BatchMode,
        paths: Vec<String>,
        source: OpenSource,
    },
    /// 进度表's 开始/结束 or 开始/结束(闪电模式) while that batch is not running: the settings
    /// dialog for the queue from its current row. An empty queue answers `Unchanged`.
    Setup { mode: BatchMode },
    /// The 批量分析 settings dialog's 开始分析 (saved as the automatic-analysis settings; the
    /// records are always saved).
    StartOrdinary { settings: AutoAnalysisSettings },
    /// The 批量精析 settings dialog's 开始分析: the move range (-1 leaves an end open, as the
    /// automatic-analysis fields it shares), 每手总计算量 (saved as `batch-analysis-playouts`)
    /// and 分析所有分支.
    StartFlash {
        start_move: i32,
        end_move: i32,
        visits: u32,
        all_branches: bool,
    },
    /// The settings dialog was closed or answered 终止分析: 批量分析 forgets its records, 批量精析
    /// keeps them in 进度表.
    CancelSetup,
    /// 取消分析 or 进度表's 开始/结束(闪电模式) while 批量精析 runs: the engine ends, the current
    /// record is not saved and stays the first row, so the batch can continue later.
    CancelFlash,
    /// 进度表's 增加棋谱: the multi-file chooser; the records join the end of the queue.
    AddFiles,
    /// 进度表's 清空: a running batch stops first (批量分析 saves its current record, 批量精析
    /// does not), then the queue is emptied.
    Clear,
    /// A row's 优先 / 上移 / 下移. The current row and the row after it cannot move up; the
    /// current row and the last row cannot move down.
    Move { item: u64, to: BatchMove },
    /// A row's 删除; the current row cannot be removed.
    Remove { item: u64 },
}

/// A queued record (a 进度表 row).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct BatchItemView {
    pub id: u64,
    /// The file name shown in the 文件名 column.
    pub name: String,
    pub path: String,
}

/// The record being analysed.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct BatchRunView {
    pub mode: BatchMode,
    /// The queued record, the first row.
    pub item: u64,
    /// The record is being read, or its engine is starting or being borrowed
    /// (进度: 引擎加载中...).
    pub loading: bool,
    /// Positions analysed so far, of `total`.
    pub done: u32,
    pub total: u32,
    /// 批量精析 borrows the foreground engine instead of an analysis process.
    pub borrowed: bool,
}

/// A record the batch could not read or could not save.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct BatchFailure {
    pub path: String,
    /// `LizzieFrame.prompt.failedToOpenFile` or `LizzieFrame.saveFileFailed`.
    pub message_key: String,
    pub detail: String,
}

/// How a batch ended, shown once.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct BatchNotice {
    pub id: u32,
    /// `Leelaz.batchAutoAnalyzeComplete`, `Leelaz.batchAutoAnalyzeStopped`,
    /// `LizzieFrame.prompt.failedToOpenFile`, `AnalysisEngine.requestDispatchFailed`,
    /// `AnalysisEngine.foregroundRestoreFailed` or `AutoAnalyze.engineUnavailable`.
    pub message_key: String,
    /// Every record of this batch that could not be read or saved, in order.
    pub failures: Vec<BatchFailure>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
pub struct BatchView {
    /// 批量分析进度表's rows: the current record first, then the records still to analyse
    /// (finished records leave the table, as `Batchfiles` from `BatchAnaNum`).
    pub items: Vec<BatchItemView>,
    /// The settings dialog to show for the queue.
    pub setup: Option<BatchMode>,
    /// The first record is being read before that mode's settings dialog (a pending start:
    /// 停止自动(批量)分析 cancels 批量分析 here).
    pub loading: Option<BatchMode>,
    pub running: Option<BatchRunView>,
    /// Increases when 进度表 should open (`openAnalysisTable` as 批量分析 of several records
    /// starts).
    pub open_table: u32,
    pub notice: Option<BatchNotice>,
}

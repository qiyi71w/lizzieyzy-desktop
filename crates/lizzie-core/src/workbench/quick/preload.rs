//! 启动Lizzie时预加载引擎 (Java `preloadConfiguredAnalysisEngineAfterStartup`,
//! `preloadQuickAnalysisEngineForKifuBrowsing`, `currentQuickAnalysisWarmupAction`): when the
//! idle analysis process starts ahead of a quick or flash analysis, and when the foreground
//! engine's search ends it again (`onMainEnginePonder`).
//!
//! A warm-up decides at once and, while it waits, again whenever the state changes (Java checks
//! every 1200 ms). It ends when it starts the process or gives up, and wherever Java stops its
//! timer: the foreground engine is switched or searches, the engine sync of a record opened or
//! waited for ends, the user pauses or unticks the option, 闪电分析 or 自动分析 starts, or the
//! lightweight model changes.

use super::super::Workbench;
use crate::engine::runtime::Phase;
use crate::record::RecordFormat;
use crate::resources as r;

/// Managed engines the analysis process waits for (`isBundledNvidiaCommand`,
/// `isBundledOpenClPath`).
const PRIMARY_ASSETS: [&str; 3] = ["windows-nvidia", "windows-tensorrt", "windows-opencl"];

/// A warm-up waiting for its conditions (Java's warm-up timer).
#[derive(Clone, Copy)]
pub(in crate::workbench) struct Warmup {
    /// The document open when the warm-up was asked for.
    doc: u64,
    /// The document whose engine sync the warm-up waited for: the end of that sync ends it.
    sync_doc: Option<u64>,
}

/// `QuickAnalysisWarmupAction`, with the wait for a record's engine sync told apart.
enum Action {
    Start,
    Wait,
    WaitForSync,
    Stop,
}

impl Workbench {
    /// 启动Lizzie时预加载引擎, after start-up and when a record download window opens or a remote
    /// compute engine becomes the foreground (`preloadQuickAnalysisEngineForKifuBrowsing`): the
    /// analysis process starts now, once the foreground engine it depends on is ready, or not at
    /// all. Nothing happens with the setting off.
    pub fn preload_analysis_engine(&mut self) {
        if !self.config.analysis_engine.preload {
            return;
        }
        self.quick.warmup = Some(Warmup { doc: self.doc.id, sync_doc: None });
        self.warmup_review();
    }

    /// A waiting warm-up decides again (`tryWarmQuickAnalysisEngineAfterPrimary`).
    pub(in crate::workbench) fn warmup_review(&mut self) {
        let Some(w) = self.quick.warmup else {
            return;
        };
        match self.warmup_action(w) {
            Action::Start => {
                self.quick.warmup = None;
                self.quick_preload();
            }
            Action::Wait => {}
            Action::WaitForSync => {
                let doc = self.doc.id;
                if let Some(w) = self.quick.warmup.as_mut() {
                    w.sync_doc = Some(doc);
                }
            }
            Action::Stop => self.quick.warmup = None,
        }
    }

    /// `stopQuickAnalysisWarmupTimer`.
    pub(in crate::workbench) fn warmup_stop(&mut self) {
        self.quick.warmup = None;
    }

    /// `currentQuickAnalysisWarmupAction` / `decideQuickAnalysisWarmup`. Java reads the setting
    /// only when the warm-up is scheduled; here unchecking it while the warm-up waits stops it.
    fn warmup_action(&self, w: Warmup) -> Action {
        let a = &self.config.analysis_engine;
        if !a.preload || !self.warmup_context_eligible() {
            return Action::Stop;
        }
        // The primary engine: the foreground one, or the one that failed in its seat.
        let foreground = self.engines.foreground();
        let failed = self.engines.foreground_failure().filter(|_| foreground.is_none());
        let primary = foreground.and_then(|f| self.engines.profile_of(f)).or(failed);
        let depends = a.reuse_current_engine
            || primary.is_some_and(|p| p.cloud.is_some())
            || primary.filter(|p| r::is_managed(p)).and_then(|p| p.managed.as_ref()).is_some_and(|m| PRIMARY_ASSETS.contains(&m.asset_id.as_str()));
        if depends && failed.is_some() {
            return Action::Stop;
        }
        // Java reads a record before the warm-up decides; an SGF record then gets an engine sync
        // (complete at once without an engine), whose end stops the warm-up (`loadFile`,
        // `scheduleEngineSyncAndResumeAfterKifuLoad`, `onSynchronized`); a GIB record gets none.
        if self.pending.is_some() {
            return Action::Wait;
        }
        if w.doc != self.doc.id && self.doc.format == Some(RecordFormat::Sgf) {
            return Action::Stop;
        }
        if self.kifu_sync_pending() {
            return Action::WaitForSync;
        }
        if w.sync_doc == Some(self.doc.id) {
            return Action::Stop;
        }
        if depends && foreground.is_none_or(|f| self.engines.phase(f) != Some(Phase::Ready)) {
            return Action::Wait;
        }
        Action::Start
    }

    /// `isQuickAnalysisWarmupContextEligible`: no pause, 自动分析 (also while it starts),
    /// 整盘精析, engine game or 人机对弈.
    fn warmup_context_eligible(&self) -> bool {
        !self.paused
            && !self.auto_analysis_starting()
            && !self.auto_analysis_active()
            && !self.whole_game_active()
            && !self.match_active()
            && !self.game_active()
    }

    /// The foreground engine searches or a stone is placed (`onMainEnginePonder`): unless a
    /// 自动分析 is starting or an automatic quick analysis runs, a waiting warm-up ends, and so
    /// does the idle local analysis process (`releaseSecondaryAnalysisResourcesForForeground`,
    /// `RELEASE_IDLE_SECONDARY`); one on an SSH host stays.
    pub(in crate::workbench) fn foreground_pondered(&mut self) {
        if self.auto_analysis_starting() || self.quick.task.is_some() {
            return;
        }
        self.warmup_stop();
        let local = self
            .quick
            .warm
            .as_ref()
            .is_some_and(|(id, _)| self.engines.worker_remote(*id).is_none());
        if local {
            self.quick_drop_warm();
        }
    }
}

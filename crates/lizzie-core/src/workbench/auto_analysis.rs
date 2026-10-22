//! Java single-record 自动分析. The ordinary foreground result path is the only engine and
//! result writer; this module only advances browsing and owns its run/confirmation/save lifecycle.
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{SaveJob, Workbench};
use crate::api::{ApiError, AutoAnalysisNotice, AutoAnalysisView};
use crate::engine::runtime::{InstanceId, Phase};
use crate::record::{DocumentEdit, NodeId, Stone, ROOT};
use crate::settings::AutoAnalysisSettings;

#[derive(Default)]
pub(super) struct AutoAnalysisFlow {
    run: Option<Run>,
    notice: Option<AutoAnalysisNotice>,
    notice_id: u32,
    pause_tip: bool,
    jobs: Vec<SaveJob>,
    saves: Vec<(u64, u64, bool, String)>,
}
struct Run {
    doc: u64,
    engine: InstanceId,
    rules: u64,
    komi: f64,
    settings: AutoAnalysisSettings,
    starting: bool,
    start: NodeId,
    pending: VecDeque<NodeId>,
    analyzed: HashSet<NodeId>,
    any: bool,
    diff: bool,
    confirmation: bool,
    diff_moves: u32,
    diff_nodes: u32,
}

impl Workbench {
    /// Java isAutoAna, including the strengthening phase and pending foreground hand-over.
    /// The forced-stop confirmation is not active (Java clears isAutoAna before asking).
    pub(super) fn auto_analysis_active(&self) -> bool {
        self.auto_analysis.run.as_ref().is_some_and(|r| !r.confirmation)
    }

    pub fn save_auto_analysis_settings(&mut self, settings: AutoAnalysisSettings) -> Result<Option<&'static str>, ApiError> {
        if settings.diff_enable {
            if !settings.diff_black && !settings.diff_white { return Ok(Some("SetDiffAnalyze.noBlackWhite")); }
            if !settings.diff_use_win && !settings.diff_use_score { return Ok(Some("SetDiffAnalyze.noWinScore")); }
            if settings.diff_time <= 0 && settings.diff_visits <= 0 && settings.diff_first_visits <= 0 {
                return Ok(Some("SetDiffAnalyze.noCondition"));
            }
        }
        self.commit_config(|c| c.auto_analysis = settings)?;
        Ok(None)
    }

    pub fn start_auto_analysis(&mut self, settings: AutoAnalysisSettings) -> Result<Result<bool, &'static str>, ApiError> {
        if self.auto_analysis.run.is_some() { return Ok(Ok(false)); }
        if !settings.black && !settings.white { return Ok(Err("SetDiffAnalyze.noBlackWhite")); }
        if !(settings.time_enabled && settings.time_seconds > 0
            || settings.visits_enabled && settings.visits > 0
            || settings.first_visits_enabled && settings.first_visits > 0) {
            return Ok(Err("SetDiffAnalyze.noCondition"));
        }
        if self.flash_active() || self.flash_releasing() { return Ok(Err("AutoAnalyze.conflict")); }
        let Some(engine) = self.engines.foreground().filter(|&f| self.engines.phase(f) == Some(Phase::Ready) && self.engines.can_analyse(f)) else {
            return Ok(Err("AutoAnalyze.engineUnavailable"));
        };
        if let Some(key) = self.save_auto_analysis_settings(settings.clone())? { return Ok(Err(key)); }
        self.auto_analysis.run = Some(Run {
            doc: self.doc.id, engine, rules: self.rules_revision, komi: self.komi,
            settings, starting: true, start: self.cursor, pending: VecDeque::new(),
            analyzed: HashSet::new(), any: false, diff: false, confirmation: false,
            diff_moves: 0, diff_nodes: 0,
        });
        self.paused = false;
        self.auto_notice("AutoAnalyze.preparingStatus", None, true);
        self.quick_hand_over();
        self.auto_analysis_poll();
        Ok(Ok(true))
    }

    /// Called after runtime events, so a borrowed quick-analysis engine is restored before use.
    pub(super) fn auto_analysis_poll(&mut self) {
        let Some(run) = &self.auto_analysis.run else { return };
        if run.doc != self.doc.id || Some(run.engine) != self.engines.foreground()
            || self.engines.phase(run.engine) != Some(Phase::Ready)
            || run.rules != self.rules_revision || run.komi != self.komi {
            self.auto_analysis_abandon();
            return;
        }
        if !run.starting || self.quick_releasing() { return; }
        if self.engines.unrestored(run.engine).is_some() {
            self.auto_analysis.run = None;
            self.auto_notice("AutoAnalyze.releaseFailed", None, false);
            return;
        }
        let s = &run.settings;
        let mut start = self.cursor;
        if s.start_move >= 0 {
            let number = s.start_move.saturating_sub(1).max(0) as u32;
            start = crate::browse::goto_move_number(&self.doc.record, self.cursor, number);
        } else if s.all_branches && !self.doc.record.is_main_trunk(start) { start = ROOT; }
        let all = s.all_branches;
        let mut pending = VecDeque::new();
        if all {
            // Java Board.analyzeAllNodesAfter: reverse children, so side branches precede trunk.
            let mut stack = vec![start];
            while let Some(node) = stack.pop() {
                pending.push_back(node);
                stack.extend(self.doc.record.node(node).children.iter().copied());
            }
            pending.pop_front();
        }
        let run = self.auto_analysis.run.as_mut().unwrap();
        run.starting = false;
        run.start = start;
        run.pending = pending;
        self.limit_stop = None;
        self.secondary_limit_stop = None;
        self.cursor = start;
        self.engines.stop_raw_net();
        self.engines.want(Some(self.analysis_target()), false);
        self.sync_engine();
    }

    /// Checks the accepted ordinary foreground packet, not a cached stronger packet. Java turns
    /// protectedCache off during automatic analysis. Time is measured by the runtime's stream.
    pub(super) fn auto_analysis_result(&mut self, node: NodeId, visits: u64, first: u64, elapsed: Duration) {
        let Some(r) = &self.auto_analysis.run else { return };
        if r.starting || r.confirmation || node != self.cursor || self.paused { return; }
        let s = &r.settings;
        let number = self.doc.record.node(node).move_number;
        if !r.diff && !s.all_branches && s.end_move >= 0 && number > s.end_move as u32 {
            self.auto_analysis_stop(false);
            return;
        }
        let side = self.doc.record.node(node).to_play;
        let skip = !r.diff && (side == Stone::Black && !s.black || side == Stone::White && !s.white);
        let (time, total, best) = if r.diff { (s.diff_time, s.diff_visits, s.diff_first_visits) } else {
            (if s.time_enabled { s.time_seconds } else { -1 }, if s.visits_enabled { s.visits } else { -1 }, if s.first_visits_enabled { s.first_visits } else { -1 })
        };
        let zen = self.engines.identity(r.engine).is_some_and(|(name, _, _)| name.eq_ignore_ascii_case("Zen")) && number < 3;
        if !(skip || zen || best > 0 && first >= best as u64 || total > 0 && visits >= total as u64 || time > 0 && elapsed >= Duration::from_secs(time as u64)) { return; }
        let (all_branches, end_move) = (s.all_branches, s.end_move);
        let r = self.auto_analysis.run.as_mut().unwrap();
        r.analyzed.insert(node);
        // Java mainline skipping calls analyzeNextMove (autoAnalysed); branch skipping doesn't.
        r.any |= !skip || !all_branches;
        if r.diff {
            if let Some(next) = r.pending.pop_front() { self.move_to(next); } else { self.auto_analysis_finish(true); }
        } else if all_branches {
            if self.doc.record.is_main_trunk(node) && (end_move >= 0 && number > end_move as u32 || self.doc.record.main_child(node).is_none()) {
                self.auto_analysis_stop(false);
            } else if let Some(next) = r.pending.pop_front() { self.move_to(next); } else { self.auto_analysis_stop(false); }
        } else if let Some(next) = self.doc.record.main_child(node) { self.move_to(next); } else { self.auto_analysis_stop(false); }
    }

    fn auto_diff_nodes(&self) -> (u32, VecDeque<NodeId>) {
        let Some(r) = &self.auto_analysis.run else { return (0, VecDeque::new()) };
        let s = &r.settings;
        let rec = &self.doc.record;
        let mut nodes = Vec::new();
        if s.all_branches {
            let mut stack = vec![r.start];
            while let Some(n) = stack.pop() { nodes.push(n); stack.extend(rec.node(n).children.iter().copied()); }
        } else {
            let mut n = r.start;
            loop {
                nodes.push(n);
                let Some(next) = rec.main_child(n).filter(|&next| s.end_move < 0 || rec.node(next).move_number <= s.end_move as u32) else { break };
                n = next;
            }
        }
        let mut count = 0;
        let mut unique = HashSet::new();
        let mut result = VecDeque::new();
        for n in nodes {
            if !r.analyzed.contains(&n) { continue; }
            let Some(parent) = rec.parent(n) else { continue };
            let mover = rec.node(n).to_play.opposite();
            if mover == Stone::Black && !s.diff_black || mover == Stone::White && !s.diff_white { continue; }
            let Some(change) = self.change(parent, n) else { continue };
            if s.diff_use_win && change.winrate.abs() >= s.diff_win_threshold || s.diff_use_score && change.score.is_some_and(|v| v.abs() >= s.diff_score_threshold) {
                count += 1;
                for node in [parent, n] { if unique.insert(node) { result.push_back(node); } }
            }
        }
        (count, result)
    }

    /// Forced stop asks once before strengthening; natural completion strengthens immediately.
    fn auto_analysis_stop(&mut self, force: bool) -> bool {
        let Some(r) = &self.auto_analysis.run else { return false };
        if r.starting { self.auto_analysis_abandon(); self.sync_engine(); return true; }
        if !r.diff && !r.confirmation && r.settings.diff_enable {
            let (count, nodes) = self.auto_diff_nodes();
            if !nodes.is_empty() {
                let r = self.auto_analysis.run.as_mut().unwrap();
                r.diff_moves = count;
                r.diff_nodes = nodes.len() as u32;
                r.pending = nodes;
                r.confirmation = force;
                self.paused = force;
                if !force { self.auto_begin_diff(); }
                self.sync_engine();
                return true;
            }
        }
        self.auto_analysis_finish(!force);
        true
    }

    pub fn stop_auto_analysis(&mut self) -> bool { self.auto_analysis_stop(true) }

    fn auto_begin_diff(&mut self) {
        let r = self.auto_analysis.run.as_mut().unwrap();
        r.confirmation = false;
        r.diff = true;
        let node = r.pending.pop_front().unwrap();
        self.paused = false;
        self.limit_stop = None;
        self.move_to(node);
        self.auto_notice("BottomToolbar.startReAnalyze", None, true);
        self.engines.want(Some(self.analysis_target()), false);
        self.sync_engine();
    }

    pub fn answer_auto_analysis_diff(&mut self, accept: bool) -> bool {
        if !self.auto_analysis.run.as_ref().is_some_and(|r| r.confirmation) { return false; }
        if accept { self.auto_begin_diff(); } else { self.auto_analysis_finish(false); }
        true
    }

    pub(super) fn auto_analysis_toggle_pause(&mut self) -> bool {
        if !self.auto_analysis_active() { return false; }
        self.paused = !self.paused;
        if self.paused { self.auto_analysis_paused(); }
        self.limit_stop = None;
        self.secondary_limit_stop = None;
        self.sync_engine();
        true
    }

    /// Space exits without the stop/save/diff path (`LizzieFrame.stopAiPlayingAndPolicy`).
    pub(super) fn auto_analysis_paused(&mut self) {
        if self.auto_analysis_active() && self.config.auto_analysis.exit_by_pause {
            self.auto_analysis.run = None;
            self.auto_analysis.pause_tip = self.config.auto_analysis.exit_tip;
        }
    }

    pub fn dismiss_auto_analysis_tip(&mut self, never_again: bool) -> Result<bool, ApiError> {
        if never_again { self.commit_config(|c| c.auto_analysis.exit_tip = false)?; }
        self.auto_analysis.pause_tip = false;
        Ok(true)
    }

    /// Document/engine/rules replacement cancels without saving or borrowing results.
    pub(super) fn auto_analysis_abandon(&mut self) { self.auto_analysis.run = None; }

    fn auto_analysis_finish(&mut self, completed: bool) {
        let Some(run) = self.auto_analysis.run.take() else { return };
        self.paused = true;
        self.sync_engine();
        if run.settings.auto_save && run.any {
            self.auto_append_summary();
            let stamp = chrono::Local::now().format("%Y%m%d%H%M%S").to_string();
            let label = super::exporting::java_text(self.active_language, "Leelaz.analyzed");
            let path = self.auto_analysis_output(&stamp, &label);
            let directory = path.parent().unwrap_or(Path::new("")).to_string_lossy().into_owned();
            let mut job = self.begin_save(&path.to_string_lossy(), "LizzieYzy Tauri");
            job.create_parent = true;
            self.auto_analysis.saves.push((job.ticket, self.doc.id, completed, directory));
            self.auto_analysis.jobs.push(job);
        } else if !run.settings.auto_save || !completed {
            self.auto_notice(if completed { "BottomToolbar.stopAutoAnaHint" } else { "BottomToolbar.stopAutoAnaStoppedHint" }, None, false);
        }
    }

    /// Java workDirectory maps to this product's per-user application data directory (the parent
    /// of its existing `save/` recovery folder), not the install folder or process cwd.
    pub fn auto_analysis_output(&self, timestamp: &str, analyzed_label: &str) -> PathBuf {
        let work = self.storage.save_dir.parent().unwrap_or(Path::new("."));
        match self.doc.path.as_deref() {
            None => work.join("AnalyzedGames").join(format!("{timestamp}.sgf")),
            Some(source) => {
                let source = Path::new(source);
                let parent = source.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(work);
                let stem = source.file_stem().unwrap_or_default().to_string_lossy();
                parent.join(format!("{stem}_{analyzed_label}_{timestamp}.sgf"))
            }
        }
    }

    /// Captured jobs go through the adapter's existing T02 serial writer, never a second writer.
    pub fn take_auto_analysis_saves(&mut self) -> Vec<SaveJob> { std::mem::take(&mut self.auto_analysis.jobs) }

    pub(super) fn auto_analysis_save_finished(&mut self, ticket: u64, success: bool) -> bool {
        let Some(index) = self.auto_analysis.saves.iter().position(|s| s.0 == ticket) else { return false };
        let (_, doc, completed, directory) = self.auto_analysis.saves.remove(index);
        if doc == self.doc.id {
            self.auto_notice(if !success { "LizzieFrame.saveFileFailed" } else if completed { "Leelaz.autoAnalyzeComplete" } else { "Leelaz.autoAnalyzeStoppedSaved" }, success.then_some(directory), false);
        }
        true
    }

    /// SGFParser.appendAiScoreBlunder runs only on an auto-save after autoAnalysed became true.
    fn auto_append_summary(&mut self) {
        let rec = &self.doc.record;
        let mut lines = Vec::new();
        // Java's NodeInfo lives on the previous position but describes its next move.
        let nodes = rec.path_to(rec.line_end(ROOT));
        for side in [Stone::Black, Stone::White] {
            let mut values: Vec<_> = nodes.iter().filter_map(|&n| self.evaluation(n))
                .filter(|e| e.mover == side && e.winrate_change.is_some()).collect();
            if values.len() < 10 { continue; }
            let score = values.iter().map(|e| e.share).sum::<f64>() * 100.0 / values.len() as f64;
            values.sort_by(|a, b| b.winrate_change.unwrap().abs().total_cmp(&a.winrate_change.unwrap().abs()));
            let (ai, top) = if side == Stone::Black {
                (super::exporting::java_text(self.active_language, "SGFParse.blackAiScore"), super::exporting::java_text(self.active_language, "SGFParse.blackTop10"))
            } else {
                (super::exporting::java_text(self.active_language, "SGFParse.whiteAiScore"), super::exporting::java_text(self.active_language, "SGFParse.whiteTop10"))
            };
            let worst = values.iter().take(10).map(|e| {
                let value = e.winrate_change.unwrap();
                format!("{}({}{:.1}%)", e.move_number, if value > 0.0 { "+" } else { "-" }, value.abs())
            }).collect::<Vec<_>>().join(", ");
            lines.push(format!("{ai}{score:.1}\n{top} {worst}"));
        }
        if !lines.is_empty() {
            let old = &rec.node(ROOT).comment;
            let comment = if old.is_empty() { lines.join("\n") } else { format!("{old}\n{}", lines.join("\n")) };
            let _ = self.edit_document(DocumentEdit::SetComment { node: ROOT, comment });
        }
    }

    fn auto_notice(&mut self, key: &str, directory: Option<String>, timed: bool) {
        self.auto_analysis.notice_id = self.auto_analysis.notice_id.wrapping_add(1);
        self.auto_analysis.notice = Some(AutoAnalysisNotice { id: self.auto_analysis.notice_id, message_key: key.into(), directory, timed });
    }

    pub(super) fn auto_analysis_view(&self) -> AutoAnalysisView {
        let r = self.auto_analysis.run.as_ref();
        AutoAnalysisView {
            active: self.auto_analysis_active(), starting: r.is_some_and(|r| r.starting),
            diff: r.is_some_and(|r| r.diff), confirmation: r.is_some_and(|r| r.confirmation),
            diff_moves: r.map_or(0, |r| r.diff_moves), diff_nodes: r.map_or(0, |r| r.diff_nodes),
            pause_tip: self.auto_analysis.pause_tip, notice: self.auto_analysis.notice.clone(),
        }
    }
}

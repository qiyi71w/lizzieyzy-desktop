//! What the foreground engine's ordinary analysis searches: 强制计算区域 (Java
//! `LizzieFrame.allowcoords` / `avoidcoords`, `Leelaz.analyzeAvoid`) and 选点重点研究
//! (`TrackingAnalysisController`, `Leelaz` move focus).
//!
//! A region restricts the candidates for both colours to (or away from) its points. It belongs to
//! the session, never to the record. One chosen on the board's right-click menu lasts for the
//! request it was sent with: the next ordinary request — another position, or a resume after the
//! analysis stopped — goes without it, unless the region is kept (`isKeepingForce` by a toolbar or
//! Alt selection, or 持续分析/不分析 `isKeepForcing`); a kept region goes with every request. Every
//! region change lets the next live result replace each stored one (`clearBestMovesAfter`), and
//! each position a kept region is sent for its own (`clearbestmoves`).
//!
//! Tracked points belong to one position of one document, one engine instance and the current
//! target and update interval: any change of these ends the tracking (`contextChanged`). While the
//! foreground engine analyses, each tracked point below the target gets extra search (`focus`)
//! until its candidate reaches the target or stops progressing for 8 seconds. Changing only the
//! focus re-sends the same request, which KataGo continues on the same search tree. Pausing ends
//! the extra search and keeps the points; nothing here resumes a paused analysis.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::Workbench;
use crate::api::{ApiError, ErrorKind, FocusView, RegionCommand, RegionView, TrackedPointView, TrackingCommand};
use crate::engine::runtime::{FocusCapability, InstanceId, SearchFocus, SearchRegion};
use crate::record::{NodeId, Point};
use crate::settings::{RegionSettings, TrackingSettings};

/// `TrackingAnalysisController.PROGRESS_TIMEOUT_MILLIS`.
const PROGRESS_TIMEOUT: Duration = Duration::from_millis(8000);

/// A document position: document, node and the node's stamp.
type PositionKey = (u64, NodeId, u64);

struct Region {
    allow: bool,
    points: Vec<Point>,
    /// The position the last request with it was for.
    anchor: PositionKey,
    /// Analysis stopped since that request; a resume is a new request.
    interrupted: bool,
    /// The depth the right-click menu sent its request with (1), until the next request.
    first_until: Option<u32>,
}

/// What tracked points are bound to (`TrackingAnalysisController.Context`).
#[derive(Clone, Copy, PartialEq)]
struct TrackContext {
    position: PositionKey,
    instance: InstanceId,
    komi_bits: u64,
    rules_rev: u64,
    target: u32,
    interval: u32,
}

/// The candidates of the foreground engine's latest analysis line, with the instance and position
/// it searched (`bestMoves`).
struct LiveLine {
    instance: InstanceId,
    position: PositionKey,
    candidates: Vec<(Point, u64)>,
}

#[derive(Default)]
pub(super) struct SearchFlow {
    region: Option<Region>,
    /// `isKeepingForce`: set by a toolbar region button or an Alt selection, cleared by 清除区域.
    keep_selection: bool,
    /// `isKeepForcing`: the right-click 持续分析/不分析 check box.
    keep_forcing: bool,
    context: Option<TrackContext>,
    selected: Vec<Point>,
    active: Vec<Point>,
    visits: HashMap<Point, u64>,
    deadlines: HashMap<Point, Instant>,
    /// What a newly tracked point counts at once; a stored result with more visits from an earlier
    /// search does not count.
    live: Option<LiveLine>,
}

impl SearchFlow {
    fn keep(&self) -> bool {
        self.keep_selection || self.keep_forcing
    }

    fn end_tracking(&mut self) {
        self.context = None;
        self.selected.clear();
        self.active.clear();
        self.visits.clear();
        self.deadlines.clear();
    }

    fn deactivate(&mut self, point: Point) {
        self.active.retain(|&p| p != point);
        self.deadlines.remove(&point);
    }

    /// `observeProgress`: the highest visits of each tracked candidate; one reaching the target
    /// stops getting extra search, one still progressing gets another 8 seconds.
    fn observe(&mut self, candidates: &[(Point, u64)]) {
        let Some(target) = self.context.map(|c| u64::from(c.target)) else {
            return;
        };
        let now = Instant::now();
        for &(point, visits) in candidates {
            if !self.selected.contains(&point) {
                continue;
            }
            let previous = self.visits.get(&point).copied().unwrap_or(0);
            self.visits.insert(point, previous.max(visits));
            if !self.active.contains(&point) {
                continue;
            }
            if visits >= target {
                self.deactivate(point);
            } else if visits > previous {
                self.deadlines.insert(point, now + PROGRESS_TIMEOUT);
            }
        }
    }

    /// Counts the live candidates for the points tracked at their position with their instance.
    fn observe_live(&mut self) {
        let Some(live) = self.live.take() else {
            return;
        };
        if self
            .context
            .is_some_and(|c| c.instance == live.instance && c.position == live.position)
        {
            self.observe(&live.candidates);
        }
        self.live = Some(live);
    }
}

impl Workbench {
    fn position_key(&self) -> PositionKey {
        (self.doc.id, self.cursor, self.doc.record.node(self.cursor).stamp)
    }

    fn on_board(&self, p: Point) -> bool {
        let size = self.doc.record.size;
        p.x < size.width && p.y < size.height
    }

    fn empty_here(&self, p: Point) -> bool {
        self.doc.record.position(self.cursor).at(p).is_none()
    }

    /// A background task holds the foreground engine (its pause belongs to the task).
    fn foreground_borrowed(&self) -> bool {
        self.quick_holds_foreground() || self.flash_holds_foreground() || self.whole_game_holds_foreground()
    }

    // -------------------------------------------------------------------- the engine's wish

    /// Brings the region and the tracking up to date with the position and what the foreground
    /// engine does, and hands both to the runtime for the next request. Called by `sync_engine`.
    pub(super) fn sync_search(&mut self, analyse: bool) {
        let key = self.position_key();
        let size = self.doc.record.size;
        let keep = self.search.keep();
        if let Some(r) = &mut self.search.region {
            r.points.retain(|p| p.x < size.width && p.y < size.height);
        }
        if self.search.region.as_ref().is_some_and(|r| r.points.is_empty()) {
            self.search.region = None;
        }
        let mut reissued = false;
        if let Some(r) = &mut self.search.region {
            if !analyse {
                r.interrupted = true;
            } else if r.anchor != key || r.interrupted {
                // A new ordinary request (`ponderCurrentPosition`): a kept region goes with it.
                r.anchor = key;
                r.interrupted = false;
                r.first_until = None;
                reissued = true;
            }
        }
        if reissued {
            if keep {
                self.store.invalidate(self.cursor);
            } else {
                self.search.region = None;
            }
        }
        let region = self.search.region.as_ref().map(|r| SearchRegion {
            allow: r.allow,
            points: r.points.clone(),
            until_move: r.first_until.unwrap_or_else(|| self.config.region.until_move(r.allow)),
        });

        let context = self.tracking_context();
        if self.search.context.is_some() && self.search.context != context {
            self.search.end_tracking();
        }
        if !analyse || !self.tracking_available() {
            self.search.active.clear();
            self.search.deadlines.clear();
        }
        // The latest line counts only while its engine still searches this position.
        let foreground = self.engines.foreground();
        if !analyse
            || self
                .search
                .live
                .as_ref()
                .is_some_and(|l| Some(l.instance) != foreground || l.position != key)
        {
            self.search.live = None;
        }
        let focus = match self.search.context {
            Some(c) if !self.search.active.is_empty() => Some(SearchFocus {
                instance: c.instance,
                doc: c.position.0,
                node: c.position.1,
                stamp: c.position.2,
                points: self.search.active.clone(),
            }),
            _ => None,
        };
        self.engines.set_search(region, focus);
    }

    // -------------------------------------------------------------------- 强制计算区域

    /// 强制计算区域 operations. Choosing a region asks for analysis (`analyzeAvoid` makes the
    /// engine ponder), unless a background task holds the foreground engine.
    pub fn region(&mut self, op: RegionCommand) -> Result<bool, ApiError> {
        let check = |wb: &Self, p: Point| {
            if wb.on_board(p) {
                Ok(())
            } else {
                Err(ApiError::new(
                    ErrorKind::InvalidArgument,
                    format!("point {p:?} is off the board"),
                ))
            }
        };
        let key = self.position_key();
        let set = |allow: bool, points: Vec<Point>, first_until: Option<u32>| Region {
            allow,
            points,
            anchor: key,
            interrupted: false,
            first_until,
        };
        let points_of = |wb: &Self, allow: bool| -> Vec<Point> {
            wb.search
                .region
                .as_ref()
                .filter(|r| r.allow == allow)
                .map(|r| r.points.clone())
                .unwrap_or_default()
        };
        match op {
            RegionCommand::Select { allow, from, to } => {
                check(self, from)?;
                check(self, to)?;
                // `selectForceAllowAvoid`: column by column, points already chosen once.
                let mut points = points_of(self, allow);
                for x in from.x.min(to.x)..=from.x.max(to.x) {
                    for y in from.y.min(to.y)..=from.y.max(to.y) {
                        let p = Point { x, y };
                        if !points.contains(&p) {
                            points.push(p);
                        }
                    }
                }
                self.search.keep_selection = true;
                self.search.region = Some(set(allow, points, None));
                self.region_chosen();
            }
            RegionCommand::Keep => {
                if self.search.keep_selection {
                    return Ok(false);
                }
                self.search.keep_selection = true;
                return Ok(true);
            }
            RegionCommand::AllowPoint { point } => {
                check(self, point)?;
                let points = if self.empty_here(point) {
                    vec![point]
                } else {
                    points_of(self, true)
                };
                self.search.region = (!points.is_empty()).then(|| set(true, points, Some(1)));
                self.region_chosen();
            }
            RegionCommand::AddAllowPoint { point } => {
                check(self, point)?;
                let mut points = points_of(self, true);
                if self.empty_here(point) && !points.contains(&point) {
                    points.push(point);
                }
                self.search.region = (!points.is_empty()).then(|| set(true, points, Some(1)));
                self.region_chosen();
            }
            RegionCommand::RemoveAllowPoint { point } => {
                check(self, point)?;
                let mut points = points_of(self, true);
                points.retain(|&p| p != point);
                self.search.region = (!points.is_empty()).then(|| set(true, points, Some(1)));
                self.region_chosen();
            }
            RegionCommand::AvoidPoint { point } => {
                check(self, point)?;
                // The engine and the board show only the avoided points from now on.
                let mut points = points_of(self, false);
                if self.empty_here(point) && !points.contains(&point) {
                    points.push(point);
                }
                self.search.region = (!points.is_empty()).then(|| set(false, points, None));
                self.region_chosen();
            }
            RegionCommand::ToggleKeepForcing => {
                self.search.keep_forcing = !self.search.keep_forcing;
            }
            RegionCommand::ClearForcing => {
                // `cancelavoid`: analysis resumes without a region.
                self.search.region = None;
                self.region_chosen();
            }
            RegionCommand::Clear => {
                // 清除区域: the stored results give way only when there was a region.
                if self.search.region.take().is_some() {
                    self.store.invalidate_all();
                }
                self.search.keep_selection = false;
            }
        }
        self.sync_engine();
        Ok(true)
    }

    /// After a region choice: stored results give way to the next live ones and the foreground
    /// engine analyses (unless a background task holds it).
    fn region_chosen(&mut self) {
        self.store.invalidate_all();
        if !self.foreground_borrowed() {
            self.paused = false;
            self.limit_stop = None;
        }
    }

    pub fn set_region_settings(&mut self, settings: RegionSettings) -> Result<bool, ApiError> {
        if self.config.region == settings {
            return Ok(false);
        }
        self.commit_config(|c| c.region = settings)?;
        self.sync_engine();
        Ok(true)
    }

    // -------------------------------------------------------------------- 选点重点研究

    fn tracking_context(&self) -> Option<TrackContext> {
        Some(TrackContext {
            position: self.position_key(),
            instance: self.engines.foreground()?,
            komi_bits: self.komi.to_bits(),
            rules_rev: self.rules_revision,
            target: self.config.tracking.max_visits.max(1),
            interval: self.config.engine.analysis.interval_centisec.max(1),
        })
    }

    /// `canStartTrackingAnalysis`: a started local KataGo in the foreground whose own probe found
    /// `focus`, analysing for the user (not lent to a background task, not 双引擎模式, not the pure
    /// network, not setting up a position).
    fn tracking_available(&self) -> bool {
        let Some(fg) = self.engines.foreground() else {
            return false;
        };
        self.engines.complete(fg)
            && self.engines.is_katago(fg)
            && self.engines.focus_capability(fg) == FocusCapability::Supported
            && self.engines.unrestored(fg).is_none()
            && !self.foreground_borrowed()
            && !self.double_engine()
            && !self.engines.raw_net_active()
            && !self.setup_mode()
    }

    /// A foreground analysis line of `instance` for `position`: it becomes the live candidates and
    /// counts for the points tracked there. Returns whether the line is the tracking's, whose
    /// search's result is shown even over a stored one with more visits (`adoptOrdinaryAnalysis`).
    pub(super) fn tracking_line(
        &mut self,
        instance: InstanceId,
        position: PositionKey,
        candidates: impl Iterator<Item = (Point, u64)>,
    ) -> bool {
        let mut candidates_now = self.search.live.take().map(|l| l.candidates).unwrap_or_default();
        candidates_now.clear();
        candidates_now.extend(candidates);
        self.search.live = Some(LiveLine {
            instance,
            position,
            candidates: candidates_now,
        });
        self.search.observe_live();
        !self.search.selected.is_empty()
            && self
                .search
                .context
                .is_some_and(|c| c.instance == instance && c.position == position)
    }

    /// 评估此点 / 取消评估 / 清除所有评估选点. Returns whether anything changed.
    pub fn tracking(&mut self, op: TrackingCommand) -> Result<bool, ApiError> {
        match op {
            TrackingCommand::Track { point } => {
                if !self.on_board(point) {
                    return Err(ApiError::new(
                        ErrorKind::InvalidArgument,
                        format!("point {point:?} is off the board"),
                    ));
                }
                if !self.tracking_available()
                    || self.search.selected.contains(&point)
                    || !self.empty_here(point)
                    || !self.trackable_in_region(point)
                {
                    return Ok(false);
                }
                let context = self.tracking_context();
                if self.search.context != context {
                    self.search.end_tracking();
                    self.search.context = context;
                }
                self.search.selected.push(point);
                self.search.active.push(point);
                self.search.visits.insert(point, 0);
                self.search.deadlines.insert(point, Instant::now() + PROGRESS_TIMEOUT);
                // The current search's candidates count at once (`beginMoveFocus`).
                self.search.observe_live();
            }
            TrackingCommand::Untrack { point } => {
                if !self.search.selected.contains(&point) {
                    return Ok(false);
                }
                self.search.selected.retain(|&p| p != point);
                self.search.visits.remove(&point);
                self.search.deactivate(point);
                if self.search.selected.is_empty() {
                    self.search.context = None;
                }
            }
            TrackingCommand::Clear => {
                if self.search.selected.is_empty() {
                    return Ok(false);
                }
                self.search.end_tracking();
            }
        }
        self.sync_engine();
        Ok(true)
    }

    /// `isAvailablePoint`: with a kept region, only its allowed points, or none of its avoided
    /// ones.
    fn trackable_in_region(&self, point: Point) -> bool {
        match &self.search.region {
            Some(r) if self.search.keep() => r.points.contains(&point) == r.allow,
            _ => true,
        }
    }

    pub fn set_tracking_settings(&mut self, settings: TrackingSettings) -> Result<bool, ApiError> {
        let settings = settings.normalized();
        if self.config.tracking == settings {
            return Ok(false);
        }
        self.commit_config(|c| c.tracking = settings)?;
        self.sync_engine();
        Ok(true)
    }

    /// When the next tracked point's 8 seconds without progress end.
    pub(super) fn tracking_deadline(&self) -> Option<Instant> {
        self.search.deadlines.values().min().copied()
    }

    /// Tracked points without progress for 8 seconds stop getting extra search (`onTimeout`).
    pub(super) fn tracking_tick(&mut self, now: Instant) -> bool {
        let expired: Vec<Point> = self
            .search
            .deadlines
            .iter()
            .filter(|(_, &at)| now >= at)
            .map(|(&p, _)| p)
            .collect();
        for &p in &expired {
            self.search.deactivate(p);
        }
        if expired.is_empty() {
            return false;
        }
        self.sync_engine();
        true
    }

    // -------------------------------------------------------------------- projection

    pub(super) fn region_view(&self) -> RegionView {
        RegionView {
            allow: self.search.region.as_ref().is_none_or(|r| r.allow),
            points: self
                .search
                .region
                .as_ref()
                .map(|r| r.points.clone())
                .unwrap_or_default(),
            keep_forcing: self.search.keep_forcing,
        }
    }

    pub(super) fn focus_view(&self) -> FocusView {
        let fg = self.engines.foreground();
        let analysis = self.store.get(&self.doc.record, self.cursor);
        let best = analysis.and_then(|a| a.candidates.iter().min_by_key(|c| c.order));
        let points = self
            .search
            .selected
            .iter()
            .map(|&point| {
                let result = analysis.and_then(|a| a.candidates.iter().find(|c| c.point == Some(point)));
                let rank = best.zip(result).map(|(b, r)| {
                    let score = r.score_lead.zip(b.score_lead).map(|(r, b)| r - b);
                    crate::display::classify(&self.config.display, r.winrate - b.winrate, score)
                });
                TrackedPointView {
                    point,
                    active: self.search.active.contains(&point),
                    visits: self.search.visits.get(&point).copied().unwrap_or(0),
                    rank,
                }
            })
            .collect();
        FocusView {
            capability: fg.map_or(FocusCapability::Unknown, |f| self.engines.focus_capability(f)),
            available: self.tracking_available(),
            target_visits: self.config.tracking.max_visits,
            points,
            cancellation_pending: fg.is_some_and(|f| {
                self.engines
                    .installed_focus(f)
                    .iter()
                    .any(|p| !self.search.active.contains(p))
            }),
        }
    }
}

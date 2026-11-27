//! The foreground engine's search parameters. KataGo: PDA (`playoutDoublingAdvantage`, 激进度)
//! and WRN (`analysisWideRootNoise`, 广度), the toolbar's two groups (`Menu.chkPDA` / `chkWRN`)
//! and the 引擎参数 window (`SetKataEngines`). Leela Zero / Sai: the four `lz-setoption`
//! options of their 引擎参数 window (`SetLeelaEngines`). The engine runtime keeps what each
//! engine confirmed and applies the start-up values; this module decides when the user's values
//! may reach the foreground engine and remembers them.

use super::Workbench;
use crate::api::{ApiError, CustomPdaView, ErrorKind, GameKind, KataEngineParamsEdit, KataParamEdit, KataParamsView};
use crate::engine::profile::{CustomPdaSettings, LeelaOptionSettings};
use crate::engine::runtime::{KataParam, LeaseAvailability};
use crate::record::{GameRecord, NodeId, NodeKind, Point, Stone, ROOT};

impl Workbench {
    /// Why the user's `param` cannot reach the foreground KataGo now, as a Java message key
    /// (`Menu.breadthAdmissionAvailable`: 野狐直连 and the engine's own state). A 人机对弈 game
    /// takes both, except 广度 in a genmove game (`clearWRNforGame` disables it); an engine match,
    /// whose groups are hidden, takes neither. HumanSL only pauses the foreground engine, which
    /// Java's toolbar still sets (`previewExclusiveGtpLeaseAvailability` has no mode checks).
    pub(super) fn kata_param_refusal(&self, param: KataParam) -> Option<&'static str> {
        if self.fox_owns_board() || self.fox_holds_engine() {
            return Some("Fox.conflict");
        }
        if self.match_active() {
            return Some("AnalysisSettings.reuseStatus.engine_game");
        }
        if param == KataParam::Wrn && self.play_kind() == Some(GameKind::Genmove) {
            return Some("AnalysisSettings.reuseStatus.play_mode");
        }
        match self.engines.exclusive_gtp_availability() {
            LeaseAvailability::Available => None,
            LeaseAvailability::NoForegroundEngine | LeaseAvailability::EngineNotReady => {
                Some("LizzieFrame.setParamNoEngineHint")
            }
            LeaseAvailability::NotKatago => Some("SetKataRules.notKataGoHint"),
            LeaseAvailability::EngineStateUnrestored => Some("AnalysisSettings.reuseStatus.engine_state_unrestored"),
            LeaseAvailability::ReadBoardGma => Some("AnalysisSettings.reuseStatus.readboard_gma"),
            LeaseAvailability::MissingCapability if param == KataParam::Pda => None,
            LeaseAvailability::MissingCapability => Some("AnalysisSettings.reuseStatus.missing_capability"),
            LeaseAvailability::ExistingLease => Some("AnalysisSettings.reuseStatus.existing_lease"),
            LeaseAvailability::PlayMode => Some("AnalysisSettings.reuseStatus.play_mode"),
            LeaseAvailability::HumanSlGame => Some("AnalysisSettings.reuseStatus.human_sl_game"),
            LeaseAvailability::ApplicationExclusive => Some("AnalysisSettings.reuseStatus.application_exclusive_mode"),
        }
    }

    pub(super) fn kata_params_view(&self) -> KataParamsView {
        let foreground = self.engines.foreground();
        let params = foreground.map(|f| self.engines.kata_params(f)).unwrap_or_default();
        let (reads, read) = (params.reads, foreground.filter(|_| params.reads > 0));
        KataParamsView {
            pda: params.pda,
            wrn: params.wrn,
            pda_unavailable: self.kata_param_refusal(KataParam::Pda).map(str::to_string),
            wrn_unavailable: self.kata_param_refusal(KataParam::Wrn).map(str::to_string),
            pda_read_back: read.map(|f| format!("{f}:{reads}")),
            wrn_read_back: read.map(|f| format!("{f}:{reads}:{}", self.play_wrn_given_back())),
        }
    }

    /// The foreground engine's PDA, which its live results are made with (`BoardData.pda`); 0
    /// until the engine reported one.
    pub(super) fn engine_pda(&self, instance: crate::engine::runtime::InstanceId) -> f64 {
        self.engines.kata_params(instance).pda.unwrap_or(0.0)
    }

    /// The breadth an engine's live `result` was made with (`BoardData.wrnKnown`/`wrn`): the WRN
    /// the engine confirmed, for KataGo data outside a genmove engine game; `None` (unknown) for
    /// other results and while the engine confirmed none.
    pub(super) fn engine_wrn(
        &self,
        instance: crate::engine::runtime::InstanceId,
        result: &crate::api::NodeAnalysisView,
    ) -> Option<f64> {
        if result.score_lead.is_none() || self.genmove_match_active() {
            return None;
        }
        self.engines.kata_params(instance).wrn
    }

    /// Toolbar 激进度 / 广度: the foreground KataGo uses `value` when `enabled`, else 0. The check
    /// and the value are remembered (the value only while checked); a 广度 change lets every
    /// stored result give way to the next live one (`clearBestMovesAfter(start)`). A refusal is
    /// returned as its message key and changes nothing.
    pub fn set_kata_param(
        &mut self,
        param: KataParam,
        enabled: bool,
        value: f64,
    ) -> Result<Option<&'static str>, ApiError> {
        if let Some(refusal) = self.kata_param_refusal(param) {
            return Ok(Some(refusal));
        }
        let value = if value.is_finite() { value } else { 0.0 };
        // A `KataGoPda` toolbar has its own value read-back (`txtPDA` beside the custom label);
        // Java's `setPda` only sends there and leaves the standard 激进度 preferences alone.
        if param == KataParam::Pda
            && self.engines.foreground().is_some_and(|f| self.engines.is_kata_go_pda(f))
        {
            self.send_kata_param(param, if enabled { value } else { 0.0 }, true);
            return Ok(None);
        }
        self.commit_config(|c| {
            let e = &mut c.engine;
            match param {
                KataParam::Pda => {
                    e.kata_pda_enabled = enabled;
                    if enabled {
                        e.kata_pda = Some(value);
                    }
                }
                KataParam::Wrn => {
                    e.kata_params.wrn_enabled = enabled;
                    if enabled {
                        e.kata_params.wrn = Some(value);
                    }
                }
            }
        })?;
        // The toolbar's 广度 lets stored results give way on every check, uncheck and edit.
        self.send_kata_param(param, if enabled { value } else { 0.0 }, true);
        Ok(None)
    }

    /// 引擎参数 window 确定 (`SetKataEngines.applyExistingSettings`): the toolbar groups' display is
    /// saved; a row's 自动加载 only when it or the value was changed (a checked 自动加载 takes the
    /// row's value for start-up); a row's 修改 and value only when they were changed, and then the
    /// foreground KataGo gets the row's value, or 0 when 修改 is off; only a checked 广度 lets stored
    /// results give way. An unchanged row never overwrites newer engine or saved values. A row
    /// whose value is not a number is saved but not sent. Needs a ready KataGo in the foreground;
    /// the values reach it only when it is free for them. While an engine match holds the two
    /// parameters (until its batch gave them back), 修改 and the values are left alone and
    /// only the preferences are saved: they change in 对局参数.
    pub fn save_kata_engine_params(&mut self, edit: KataEngineParamsEdit) -> Result<Option<&'static str>, ApiError> {
        if self.fox_owns_board() || self.fox_holds_engine() {
            return Ok(Some("Fox.conflict"));
        }
        let engine_game = self.match_active();
        if !engine_game {
            match self.engines.exclusive_gtp_availability() {
                LeaseAvailability::NoForegroundEngine | LeaseAvailability::EngineNotReady => {
                    return Ok(Some("LizzieFrame.setParamNoEngineHint"));
                }
                LeaseAvailability::NotKatago => return Ok(Some("SetKataRules.notKataGoHint")),
                _ => {}
            }
        }
        let finite = |row: KataParamEdit| KataParamEdit {
            value: row.value.filter(|v| v.is_finite()),
            edited: row.edited && !engine_game,
            ..row
        };
        let (pda, wrn) = (edit.pda.map(finite), finite(edit.wrn));
        self.commit_config(|c| {
            let e = &mut c.engine;
            e.kata_params.show_pda = edit.show_pda;
            e.kata_params.show_wrn = edit.show_wrn;
            if let Some(row) = pda {
                if row.edited {
                    e.kata_pda_enabled = row.enabled;
                    e.kata_pda = row.value;
                }
                if row.auto_load_edited {
                    e.kata_params.auto_load_pda = row.enabled && row.auto_load;
                    if row.enabled && row.auto_load {
                        e.kata_params.auto_load_pda_value = row.value;
                    }
                }
            }
            if wrn.edited {
                e.kata_params.wrn_enabled = wrn.enabled;
                e.kata_params.wrn = wrn.value;
            }
            if wrn.auto_load_edited {
                e.kata_params.auto_load_wrn = wrn.enabled && wrn.auto_load;
                if wrn.enabled && wrn.auto_load {
                    e.kata_params.auto_load_wrn_value = wrn.value;
                }
            }
        })?;
        for (param, row) in [(KataParam::Pda, pda), (KataParam::Wrn, Some(wrn))] {
            let Some(row) = row.filter(|r| r.edited && self.kata_param_refusal(param).is_none()) else { continue };
            let value = if row.enabled { row.value } else { Some(0.0) };
            if let Some(value) = value {
                self.send_kata_param(param, value, row.enabled);
            }
        }
        Ok(None)
    }

    /// 自定义内容's 激进度 / 广度 (`ToolbarParameterToggles.show`): saves whether the toolbar shows
    /// the group, which the 引擎参数 window's 在工具栏中控制 shows and saves too. Whether the group
    /// can show (a foreground KataGo, no engine match) is the toolbar's; nothing reaches an engine.
    pub fn set_kata_param_shown(&mut self, param: KataParam, shown: bool) -> Result<bool, ApiError> {
        let saved = &self.config.engine.kata_params;
        let saved = match param {
            KataParam::Pda => saved.show_pda,
            KataParam::Wrn => saved.show_wrn,
        };
        if saved == shown {
            return Ok(false);
        }
        self.commit_config(|c| match param {
            KataParam::Pda => c.engine.kata_params.show_pda = shown,
            KataParam::Wrn => c.engine.kata_params.show_wrn = shown,
        })?;
        Ok(true)
    }

    /// Leela Zero / Sai 引擎参数 window 确定 (`SetLeelaEngines`): the four rows are saved as they
    /// are, then each row with 修改 checked and a value goes to the foreground engine at once
    /// (`lz-setoption`), which searches again; a row with 修改 and 自动加载 is set on every Leela
    /// Zero / Sai start. Needs a ready Leela Zero / Sai in the foreground (another engine than
    /// KataGo gets `LizzieFrame.setParamsWarning`); the values reach it only when it is free.
    pub fn save_leela_engine_options(&mut self, options: LeelaOptionSettings) -> Result<Option<&'static str>, ApiError> {
        if self.fox_owns_board() || self.fox_holds_engine() {
            return Ok(Some("Fox.conflict"));
        }
        if matches!(
            self.engines.exclusive_gtp_availability(),
            LeaseAvailability::NoForegroundEngine | LeaseAvailability::EngineNotReady
        ) {
            return Ok(Some("LizzieFrame.setParamNoEngineHint"));
        }
        if !self.engines.foreground().is_some_and(|f| self.engines.takes_lz_options(f)) {
            return Ok(Some("LizzieFrame.setParamsWarning"));
        }
        let edited = options.edited();
        self.commit_config(|c| c.engine.leela_options = options)?;
        self.engines.set_leela_options(&edited);
        Ok(None)
    }

    /// Sends `value`; a 广度 the engine took lets stored results give way when `clear`, and a
    /// game that gives 广度 back at its end gives back this value.
    fn send_kata_param(&mut self, param: KataParam, value: f64, clear: bool) {
        if !self.engines.set_kata_param(param, value) || param != KataParam::Wrn {
            return;
        }
        self.play_wrn_changed(value);
        if clear {
            self.invalidate_analysis(crate::api::AnalysisScope::All);
        }
    }

    /// The foreground KataGoPda's reported PDA / cap and live mode; saved mode before read-back.
    pub(super) fn custom_pda_view(&self) -> CustomPdaView {
        let (pda, cap, mode) = self
            .engines
            .foreground()
            .map(|f| self.engines.custom_pda(f))
            .unwrap_or((None, None, None));
        CustomPdaView { pda, cap, mode: mode.unwrap_or(self.config.engine.custom_pda.mode) }
    }

    /// KataGoPda 激进参数PDA设置 确定 (`SetKataPDA`): the mode, its value and 自动套用 are saved;
    /// only a `KataGoPda` receives `pda` / `dympdacap`: during an engine game its participant
    /// (`kataGoPdaEngineGameParticipantOrPrimary`), otherwise the foreground engine. An empty
    /// value sends nothing. The dialog opens and saves on any engine (the fixed Java build could
    /// not tell whether the engine understands these commands). A 动态 系数 of zero or a
    /// non-finite value is `SetKataPDA.wrongParameter` and changes nothing.
    pub fn save_custom_pda(&mut self, settings: CustomPdaSettings) -> Result<Option<&'static str>, ApiError> {
        if settings.invalid() {
            return Ok(Some("SetKataPDA.wrongParameter"));
        }
        self.commit_config(|c| c.engine.custom_pda = settings.clone())?;
        // 野狐 owns the search; the settings are kept for the next start instead of changing
        // the engine under it.
        if !self.fox_owns_board() && !self.fox_holds_engine() {
            match self.match_pda_participant() {
                Some(id) => self.engines.apply_custom_pda_to(id, &settings, false),
                None => self.engines.apply_custom_pda(&settings, false),
            };
        }
        Ok(None)
    }

    /// 优先分析此点 (`RightClickMenu.priority`): records an empty, legal point of the shown
    /// position (the list is session only and cleared by a move or navigation), starts analysis
    /// when it was paused, and sends `setmaxpolicy <vertex> 1.1` to a `KataGoCustom` /
    /// `KataGoPda` foreground engine. Other engines and points change nothing.
    pub fn set_priority_point(&mut self, point: Point) -> Result<bool, ApiError> {
        if self.fox_owns_board() || self.fox_holds_engine() {
            return Err(ApiError::new(ErrorKind::Unavailable, "Fox.conflict"));
        }
        let Some(fg) = self.engines.foreground() else { return Ok(false) };
        if !self.engines.supports_custom_policy(fg) {
            return Ok(false);
        }
        let (size, to_play, position) = {
            let rec = &self.doc.record;
            (rec.size, rec.node(self.cursor).to_play, rec.position(self.cursor))
        };
        if !size.contains(point) || position.at(point).is_some() {
            return Ok(false);
        }
        // A suicide outside 允许自杀 is not a legal point.
        let mut trial = position.clone();
        if trial.play(to_play, point) > 0 && !self.play_rules().allow_suicide {
            return Ok(false);
        }
        let was_paused = self.paused || self.limit_stop.is_some();
        if was_paused {
            self.toggle_analysis()?;
        }
        let sent = self.engines.set_priority_point(point, size);
        if sent {
            if self.priority_engine != Some(fg) {
                self.priority_points.clear();
            }
            if !self.priority_points.contains(&point) {
                self.priority_points.push(point);
            }
            self.priority_engine = Some(fg);
            self.priority_at = Some((self.doc.id, self.cursor));
        }
        Ok(was_paused || sent)
    }

    /// 清除优先分析 (`RightClickMenu.clearPriority`): `clearpolicy`, and the session's points.
    pub fn clear_priority_points(&mut self) -> Result<bool, ApiError> {
        let had = !self.priority_points.is_empty();
        self.priority_points.clear();
        self.priority_engine = None;
        self.priority_at = None;
        if self.fox_owns_board() || self.fox_holds_engine() {
            return Ok(had);
        }
        let sent = self.engines.clear_priority_points();
        Ok(had || sent)
    }

    /// The 优先分析此点 points of `node` (`LizzieFrame.priorityMoveCoords`): recorded there for the
    /// current foreground engine; none elsewhere.
    pub(super) fn priority_points_at(&self, node: NodeId) -> &[Point] {
        if self.priority_engine == self.engines.foreground() && self.priority_at == Some((self.doc.id, node)) {
            &self.priority_points
        } else {
            &[]
        }
    }

    /// 自动套用 on a handicap game's first White move (`BoardHistoryNode` →
    /// `Leelaz.maybeAjustPDA`): the saved `AutoPDA` command again. Only a `KataGoPda` engine
    /// receives it, and only while 自动套用 is checked.
    pub(super) fn custom_pda_after_move(&mut self, node: NodeId) {
        let settings = &self.config.engine.custom_pda;
        if !settings.auto_apply {
            return;
        }
        let Some(fg) = self.engines.foreground() else { return };
        if !self.engines.is_kata_go_pda(fg) || !first_white_node_with_handicap(&self.doc.record, node) {
            return;
        }
        let settings = settings.clone();
        self.engines.apply_custom_pda(&settings, true);
    }
}

/// `Board.isFirstWhiteNodeWithHandicap`: a White action with no earlier White action and more
/// than one Black action before it. The fixed Java build keeps its handicap stones as history
/// actions; this product keeps them as root setup, so the setup/HA count stands in for them.
fn first_white_node_with_handicap(rec: &GameRecord, node: NodeId) -> bool {
    let white_action = match rec.node(node).kind {
        NodeKind::Move { color, .. } | NodeKind::Pass { color } => color == Stone::White,
        _ => return false,
    };
    if !white_action {
        return false;
    }
    let root = rec.node(ROOT);
    let mut black_actions = root.setup.add_black.len().max(rec.info.handicap as usize);
    let mut current = node;
    while let Some(parent) = rec.parent(current) {
        current = parent;
        match rec.node(current).kind {
            NodeKind::Move { color: Stone::White, .. } | NodeKind::Pass { color: Stone::White } => return false,
            NodeKind::Move { color: Stone::Black, .. } | NodeKind::Pass { color: Stone::Black } => {
                black_actions += 1;
            }
            _ => {}
        }
    }
    black_actions > 1
}

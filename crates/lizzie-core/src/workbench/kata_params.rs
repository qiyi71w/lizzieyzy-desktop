//! The foreground KataGo's PDA (`playoutDoublingAdvantage`, 激进度) and WRN
//! (`analysisWideRootNoise`, 广度): the toolbar's two groups (`Menu.chkPDA` / `chkWRN`) and the
//! 引擎参数 window (`SetKataEngines`). The engine runtime keeps what each engine confirmed; this
//! module decides when the user's values may reach the foreground engine and remembers them.

use super::Workbench;
use crate::api::{ApiError, GameKind, KataEngineParamsEdit, KataParamEdit, KataParamsView};
use crate::engine::runtime::{KataParam, LeaseAvailability};

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

    /// 引擎参数 window 确定 (`SetKataEngines.applyExistingSettings`): the toolbar groups' display,
    /// each row's 修改 / value / 自动加载 are saved (a checked 自动加载 takes the row's value for
    /// start-up), then the foreground KataGo gets each row's value, or 0 when 修改 is off; only a
    /// checked 广度 lets stored results give way. A row whose value is not a number is saved but
    /// not sent. Needs a ready KataGo in the foreground; the values reach it only when it is free
    /// for them.
    pub fn save_kata_engine_params(&mut self, edit: KataEngineParamsEdit) -> Result<Option<&'static str>, ApiError> {
        if self.fox_owns_board() || self.fox_holds_engine() {
            return Ok(Some("Fox.conflict"));
        }
        match self.engines.exclusive_gtp_availability() {
            LeaseAvailability::NoForegroundEngine | LeaseAvailability::EngineNotReady => {
                return Ok(Some("LizzieFrame.setParamNoEngineHint"));
            }
            LeaseAvailability::NotKatago => return Ok(Some("SetKataRules.notKataGoHint")),
            _ => {}
        }
        let finite = |row: KataParamEdit| KataParamEdit {
            value: row.value.filter(|v| v.is_finite()),
            ..row
        };
        let (pda, wrn) = (edit.pda.map(finite), finite(edit.wrn));
        self.commit_config(|c| {
            let e = &mut c.engine;
            e.kata_params.show_pda = edit.show_pda;
            e.kata_params.show_wrn = edit.show_wrn;
            if let Some(row) = pda {
                e.kata_pda_enabled = row.enabled;
                e.kata_pda = row.value;
                e.kata_params.auto_load_pda = row.enabled && row.auto_load;
                if row.enabled && row.auto_load {
                    e.kata_params.auto_load_pda_value = row.value;
                }
            }
            e.kata_params.wrn_enabled = wrn.enabled;
            e.kata_params.wrn = wrn.value;
            e.kata_params.auto_load_wrn = wrn.enabled && wrn.auto_load;
            if wrn.enabled && wrn.auto_load {
                e.kata_params.auto_load_wrn_value = wrn.value;
            }
        })?;
        for (param, row) in [(KataParam::Pda, pda), (KataParam::Wrn, Some(wrn))] {
            let Some(row) = row.filter(|_| self.kata_param_refusal(param).is_none()) else { continue };
            let value = if row.enabled { row.value } else { Some(0.0) };
            if let Some(value) = value {
                self.send_kata_param(param, value, row.enabled);
            }
        }
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
            self.clear_analysis_cache(crate::api::AnalysisScope::All);
        }
    }
}

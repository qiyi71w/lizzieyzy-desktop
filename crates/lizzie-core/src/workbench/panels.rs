//! Fixed Java workbench presentation; no DOM, window handles or drag drafts live here.
use super::Workbench;
use crate::api::{ApiError, ErrorKind};
use crate::settings::{BottomToolbar, CustomLayout, ExtraMode, IndependentBoard, PanelSettings, PanelSizes, WindowPlacement, WorkbenchPanel};

impl PanelSettings {
    /// Config.java:2538-2546: 大胜率图 hides, but does not forget, the list preference.
    fn list_visible(&self, mode: ExtraMode) -> bool {
        self.show_list_pane && match mode {
            ExtraMode::FourSub | ExtraMode::Thinking | ExtraMode::FloatBoard => true,
            ExtraMode::Normal | ExtraMode::Min => !self.large_winrate_graph,
            ExtraMode::DoubleEngine => false,
        }
    }

    /// Config.java:2567-2594. Leaving 大胜率图 restores only the sub board it hid.
    fn toggle_large_winrate(&mut self) {
        if !self.large_winrate_graph && self.large_sub_board {
            // toggleLargeSubBoard first makes a hidden sub board visible; in that case its
            // large flag remains set (Config.java:2599-2602).
            if !self.show_sub_board {
                self.show_sub_board = true;
            } else {
                self.large_sub_board = false;
            }
            self.large_winrate_graph = true;
        } else {
            self.large_winrate_graph = !self.large_winrate_graph;
        }
        if self.large_winrate_graph {
            if self.show_variation_graph && self.show_sub_board {
                self.show_sub_board = false;
                self.hide_sub_board_from_large_winrate = true;
            }
        } else if self.hide_sub_board_from_large_winrate && !self.show_sub_board {
            self.show_sub_board = true;
            self.hide_sub_board_from_large_winrate = false;
        }
    }
}

impl Workbench {
    /// Config.java:2530-2667,3915-3931. A single committed operation owns the interplay between
    /// large panels, list visibility and mode policy, irrespective of menu or shortcut entry.
    pub fn toggle_panel(&mut self, panel: WorkbenchPanel) -> Result<bool, ApiError> {
        let mut next = self.config.panels.clone();
        let mut mode = self.config.ui.extra_mode;
        match panel {
            WorkbenchPanel::SubBoard => next.show_sub_board = !next.show_sub_board,
            WorkbenchPanel::WinrateGraph => next.show_winrate_graph = !next.show_winrate_graph,
            WorkbenchPanel::LargeWinrateGraph => next.toggle_large_winrate(),
            WorkbenchPanel::Comment => next.show_comment = !next.show_comment,
            WorkbenchPanel::VariationGraph => next.show_variation_graph = !next.show_variation_graph,
            WorkbenchPanel::ListPane => {
                next.show_list_pane = !next.list_visible(mode);
                if next.show_list_pane && !matches!(mode, ExtraMode::FourSub | ExtraMode::Thinking | ExtraMode::FloatBoard) {
                    if mode == ExtraMode::DoubleEngine {
                        // LizzieFrame.defaultMode (3501-3530) restores its standard panels.
                        mode = ExtraMode::Normal;
                        next.classic_mode = false;
                        next.show_sub_board = true;
                        next.show_winrate_graph = true;
                        next.show_comment = true;
                        next.show_captured = true;
                        next.show_variation_graph = true;
                        next.large_sub_board = false;
                    }
                    // toggleShowListPane clears 大胜率图 directly, not its restoration policy.
                    next.large_winrate_graph = false;
                }
            }
            WorkbenchPanel::Captured => next.show_captured = !next.show_captured,
            WorkbenchPanel::Status => next.show_status = next.classic_mode || !next.show_status,
            WorkbenchPanel::LargeSubBoard => {
                if next.large_winrate_graph {
                    next.toggle_large_winrate();
                }
                if !next.show_sub_board {
                    next.show_sub_board = true;
                    next.large_sub_board = true;
                } else {
                    next.large_sub_board = !next.large_sub_board;
                }
            }
            WorkbenchPanel::AlwaysOnTop => next.always_on_top = !next.always_on_top,
        }
        if mode == ExtraMode::Min && ((panel == WorkbenchPanel::WinrateGraph && next.show_winrate_graph)
            || (panel == WorkbenchPanel::Comment && next.show_comment)
            || (panel == WorkbenchPanel::ListPane && next.show_list_pane)
            || (panel == WorkbenchPanel::VariationGraph && next.show_variation_graph)
            || (panel == WorkbenchPanel::Captured && next.show_captured)
            || (matches!(panel, WorkbenchPanel::SubBoard | WorkbenchPanel::LargeSubBoard) && next.show_sub_board)) {
            // Config.toggleExtraMode(0), not defaultMode: do not restore unrelated flags.
            mode = ExtraMode::Normal;
        }
        if panel == WorkbenchPanel::Status && next.show_status { next.classic_mode = false; }
        if mode.hides_comment() {
            next.show_comment = false;
        }
        if next == self.config.panels && mode == self.config.ui.extra_mode {
            return Ok(false);
        }
        let mode_changed = mode != self.config.ui.extra_mode;
        self.commit_config(|c| {
            c.panels = next;
            c.ui.extra_mode = mode;
        })?;
        if mode_changed {
            self.sync_engine();
        }
        Ok(true)
    }

    /// ConfigDialog2 saves its uncommitted display-page draft only with 保存设置.
    pub fn set_panel_settings(&mut self, mut settings: PanelSettings) -> Result<bool, ApiError> {
        if self.config.ui.extra_mode.hides_comment() {
            settings.show_comment = false;
        }
        if settings == self.config.panels {
            return Ok(false);
        }
        self.commit_config(|c| c.panels = settings)?;
        Ok(true)
    }

    /// LizzieFrame.java:7976-8019. A drag commits the complete size snapshot, not intermediate
    /// motion. Restore the previous projection if writing fails, as commit_config does.
    pub fn set_panel_sizes(&mut self, sizes: PanelSizes) -> Result<bool, ApiError> {
        if sizes.board_position_proportion > 8
            || [sizes.leftover_left_share, sizes.comment_height_share, sizes.variation_graph_share]
                .into_iter().flatten().any(|n| !n.is_finite() || !(0.0..=1.0).contains(&n))
        {
            return Err(ApiError::new(ErrorKind::InvalidArgument, "panel sizes"));
        }
        if self.persisted.panel_sizes == sizes {
            return Ok(false);
        }
        let old = self.persisted.panel_sizes;
        self.persisted.panel_sizes = sizes;
        if let Err(error) = self.save_persisted() {
            self.persisted.panel_sizes = old;
            return Err(error);
        }
        Ok(true)
    }

    /// Input.java:785-794 / LizzieFrame.java:8025-8035: bracket keys preserve an existing fine
    /// share and otherwise nudge the discrete proportion, bounded to 0..8.
    pub fn nudge_board_position(&mut self, delta: i32) -> Result<bool, ApiError> {
        let mut sizes = self.persisted.panel_sizes;
        if let Some(share) = sizes.leftover_left_share {
            let share = (share + f64::from(delta) / 8.0).clamp(0.0, 1.0);
            sizes.leftover_left_share = Some(share);
            sizes.board_position_proportion = (share * 8.0).round() as u8;
        } else {
            let next = i32::from(sizes.board_position_proportion).saturating_add(delta);
            if !(0..=8).contains(&next) {
                return Ok(false);
            }
            sizes.board_position_proportion = next as u8;
        }
        self.set_panel_sizes(sizes)
    }

    pub fn restore_default_panel_sizes(&mut self) -> Result<bool, ApiError> {
        self.set_panel_sizes(PanelSizes::default())
    }

    /// Menu.java:4532-4588: bottom visibility and detailed form are remembered as window
    /// presentation (`Config.java:3113`), independently of domain settings.
    pub fn set_bottom_toolbar(&mut self, mode: BottomToolbar) -> Result<bool, ApiError> {
        if self.persisted.bottom_toolbar == mode {
            return Ok(false);
        }
        let old = self.persisted.bottom_toolbar;
        self.persisted.bottom_toolbar = mode;
        if let Err(error) = self.save_persisted() {
            self.persisted.bottom_toolbar = old;
            return Err(error);
        }
        Ok(true)
    }

    /// LizzieFrame.java:3284-3535. Mode entry is a complete operation, also when reselected.
    pub fn set_extra_mode(&mut self, mode: ExtraMode) -> Result<bool, ApiError> {
        let mut p = self.config.panels.clone();
        match mode {
            ExtraMode::Normal => {
                p.classic_mode = false;
                p.show_sub_board = true;
                p.show_list_pane = true;
                p.show_winrate_graph = true;
                p.show_comment = true;
                p.show_captured = true;
                p.show_variation_graph = true;
                p.large_sub_board = false;
                p.large_winrate_graph = false;
            }
            ExtraMode::DoubleEngine | ExtraMode::Min => {
                p.show_sub_board = false;
                p.show_comment = false;
                p.show_captured = false;
                p.show_variation_graph = false;
                p.show_winrate_graph = false;
                if mode == ExtraMode::Min { p.show_list_pane = false; p.classic_mode = false; }
            }
            ExtraMode::FourSub | ExtraMode::Thinking => {
                p.show_sub_board = true;
                p.show_winrate_graph = true;
                p.show_comment = false;
                p.show_captured = true;
                p.show_variation_graph = true;
                p.show_list_pane = true;
                if mode == ExtraMode::Thinking {
                    p.large_winrate_graph = false;
                    p.large_sub_board = true;
                }
            }
            ExtraMode::FloatBoard => {
                p.show_sub_board = false;
                p.show_winrate_graph = true;
                p.show_comment = true;
                p.show_captured = true;
                p.show_variation_graph = true;
                p.show_list_pane = true;
                p.showing_independent_main = true;
                p.showing_independent_sub = true;
            }
        }
        if self.config.ui.extra_mode == mode && self.config.panels == p { return Ok(false); }
        self.commit_config(|c| { c.ui.extra_mode = mode; c.panels = p; })?;
        self.sync_engine();
        Ok(true)
    }

    pub fn set_classic_mode(&mut self) -> Result<bool, ApiError> {
        let mut p = self.config.panels.clone();
        p.classic_mode = true;
        p.show_variation_graph = false;
        p.show_sub_board = true;
        p.show_winrate_graph = true;
        p.large_winrate_graph = false;
        p.large_sub_board = true;
        p.show_comment = false;
        p.show_captured = true;
        if self.config.ui.extra_mode == ExtraMode::Normal && self.config.panels == p { return Ok(false); }
        self.commit_config(|c| { c.ui.extra_mode = ExtraMode::Normal; c.panels = p; })?;
        self.sync_engine();
        Ok(true)
    }

    /// Alt+Q / Alt+A (`toggleShowIndependMainBoard` / `toggleIndependentSubBoard`).
    pub fn toggle_independent_board(&mut self, board: IndependentBoard) -> Result<bool, ApiError> {
        self.commit_config(|c| match board {
            IndependentBoard::Main if c.panels.showing_independent_main && c.ui.extra_mode == ExtraMode::FloatBoard => {
                c.ui.extra_mode = ExtraMode::Normal;
            }
            IndependentBoard::Main => c.panels.showing_independent_main = !c.panels.showing_independent_main,
            IndependentBoard::Sub => c.panels.showing_independent_sub = !c.panels.showing_independent_sub,
        })?;
        self.sync_engine();
        Ok(true)
    }

    /// Native close differs from the panel toggle (IndependentMainBoard.java:194-196).
    pub fn close_independent_board(&mut self, board: IndependentBoard) -> Result<bool, ApiError> {
        let floating = self.config.ui.extra_mode == ExtraMode::FloatBoard;
        self.commit_config(|c| match board {
            IndependentBoard::Main => {
                c.panels.showing_independent_main = false;
                if floating {
                    c.ui.extra_mode = ExtraMode::Normal;
                    c.panels.classic_mode = false;
                    c.panels.show_sub_board = true;
                    c.panels.show_list_pane = true;
                    c.panels.show_winrate_graph = true;
                    c.panels.show_comment = true;
                    c.panels.show_captured = true;
                    c.panels.show_variation_graph = true;
                    c.panels.large_sub_board = false;
                    c.panels.large_winrate_graph = false;
                }
            }
            IndependentBoard::Sub => {
                c.panels.showing_independent_sub = false;
                if floating { c.panels.show_sub_board = true; }
            }
        })?;
        if floating && board == IndependentBoard::Main { self.sync_engine(); }
        Ok(true)
    }

    pub fn record_independent_board(&mut self, board: IndependentBoard, placement: Option<WindowPlacement>) -> Result<(), ApiError> {
        if self.persist_reset || placement.is_none() { return Ok(()); }
        let target = match board {
            IndependentBoard::Main => &mut self.persisted.independent_main_board,
            IndependentBoard::Sub => &mut self.persisted.independent_sub_board,
        };
        let previous = std::mem::replace(target, placement);
        if let Err(error) = self.save_persisted() {
            match board {
                IndependentBoard::Main => self.persisted.independent_main_board = previous,
                IndependentBoard::Sub => self.persisted.independent_sub_board = previous,
            }
            return Err(error);
        }
        Ok(())
    }

    /// SetCustomMode 确定 (SetCustomMode.java; Config.savePanelConfig, then saveCustomMode for a
    /// slot): the dialog's mode (published without the mode's entry defaults), panel flags and
    /// board position, and for a 自定义布局 slot its snapshot, saved as one change. A failed write
    /// keeps every committed value and projection.
    pub fn commit_layout(&mut self, mode: ExtraMode, mut panels: PanelSettings, board_position_proportion: u8, save: Option<(u8, Option<WindowPlacement>)>) -> Result<bool, ApiError> {
        if board_position_proportion > 8 {
            return Err(ApiError::new(ErrorKind::InvalidArgument, "board position proportion must be 0..8"));
        }
        let slot = save.map(|(slot, window)| custom_index(slot).map(|index| (index, window))).transpose()?;
        if mode.hides_comment() {
            panels.show_comment = false;
        }
        let mut next = self.config.clone();
        let mut persisted = self.persisted.clone();
        next.ui.extra_mode = mode;
        next.panels = panels;
        // The 主棋盘位置 slider (setBoardPositionProportion) acts only when it was moved.
        if persisted.panel_sizes.board_position_proportion != board_position_proportion {
            set_board_position(&mut persisted.panel_sizes, board_position_proportion);
        }
        if let Some((index, window)) = slot {
            next.custom_layouts[index] = Some(CustomLayout {
                panels: next.panels.clone(),
                board_position_proportion,
                window,
                independent_main: persisted.independent_main_board,
                independent_sub: persisted.independent_sub_board,
            });
        }
        if next == self.config && persisted == self.persisted {
            return Ok(false);
        }
        let mode_changed = next.ui.extra_mode != self.config.ui.extra_mode;
        self.commit_layout_files(next, persisted)?;
        if mode_changed {
            self.sync_engine();
        }
        Ok(true)
    }

    pub fn apply_custom_layout(&mut self, slot: u8) -> Result<bool, ApiError> {
        let index = custom_index(slot)?;
        let Some(layout) = self.config.custom_layouts[index].clone() else { return Ok(false); };
        // Config.loadCustomLayout publishes Normal or Float_Board; mode-entry defaults are not run.
        let mut next = self.config.clone();
        let mut persisted = self.persisted.clone();
        {
            let p = &mut next.panels;
            p.show_sub_board = layout.panels.show_sub_board;
            p.show_winrate_graph = layout.panels.show_winrate_graph;
            p.show_comment = layout.panels.show_comment;
            p.show_status = layout.panels.show_status;
            p.show_variation_graph = layout.panels.show_variation_graph;
            p.show_captured = layout.panels.show_captured;
            p.show_list_pane = layout.panels.show_list_pane;
            p.large_sub_board = layout.panels.large_sub_board;
            p.large_winrate_graph = layout.panels.large_winrate_graph;
            next.ui.extra_mode = if layout.panels.showing_independent_main { ExtraMode::FloatBoard } else { ExtraMode::Normal };
            // Java only opens windows requested by a snapshot; it does not close existing extras.
            p.showing_independent_main |= layout.panels.showing_independent_main;
            p.showing_independent_sub |= layout.panels.showing_independent_sub;
        }
        // Config.loadCustomLayout → setBoardPositionProportion.
        set_board_position(&mut persisted.panel_sizes, layout.board_position_proportion);
        if layout.panels.showing_independent_main { persisted.independent_main_board = layout.independent_main; }
        if layout.panels.showing_independent_sub { persisted.independent_sub_board = layout.independent_sub; }
        self.commit_layout_files(next, persisted)?;
        self.sync_engine();
        Ok(true)
    }

    /// Settings and window presentation as one write: the projection changes only once both files
    /// are saved (`save_both` puts the settings file back when the second write fails).
    fn commit_layout_files(&mut self, next: crate::settings::Config, persisted: crate::settings::Persisted) -> Result<(), ApiError> {
        crate::settings::save_both(&self.storage, &next, (!self.persist_reset).then_some(&persisted))
            .map_err(|e| ApiError::new(ErrorKind::Io, format!("saving layout failed: {e}")))?;
        self.config = next;
        self.persisted = persisted;
        Ok(())
    }
}

/// LizzieFrame.setBoardPositionProportion (LizzieFrame.java:8038-8042) with
/// InFrameLayout.leftoverShareAfterAssignedProportion (InFrameLayout.java:15-20): a dragged fine
/// share, if any, moves to the chosen position's own share, proportion / 8.
fn set_board_position(sizes: &mut PanelSizes, proportion: u8) {
    sizes.board_position_proportion = proportion;
    sizes.leftover_left_share = sizes.leftover_left_share.map(|_| f64::from(proportion) / 8.0);
}

fn custom_index(slot: u8) -> Result<usize, ApiError> {
    match slot { 1 | 2 => Ok(usize::from(slot - 1)), _ => Err(ApiError::new(ErrorKind::InvalidArgument, "custom layout slot must be 1 or 2")) }
}

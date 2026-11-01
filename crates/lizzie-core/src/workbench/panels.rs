//! Fixed Java workbench presentation; no DOM, window handles or drag drafts live here.
use super::Workbench;
use crate::api::{ApiError, ErrorKind};
use crate::settings::{BottomToolbar, ExtraMode, PanelSettings, PanelSizes, WorkbenchPanel};

impl PanelSettings {
    /// Config.java:2538-2546: 大胜率图 hides, but does not forget, the list preference.
    fn list_visible(&self, mode: ExtraMode) -> bool {
        self.show_list_pane && mode == ExtraMode::Normal && !self.large_winrate_graph
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
                if next.show_list_pane {
                    if mode != ExtraMode::Normal {
                        // LizzieFrame.defaultMode (3501-3530) restores its standard panels.
                        mode = ExtraMode::Normal;
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
            WorkbenchPanel::Status => next.show_status = !next.show_status,
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
        if mode == ExtraMode::DoubleEngine {
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
        if self.config.ui.extra_mode == ExtraMode::DoubleEngine {
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
}

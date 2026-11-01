//! Java's workbench presentation through the same operations used by menus and shortcuts.
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::settings::{BottomToolbar, ExtraMode, PanelSettings, PanelSizes, Storage, WorkbenchPanel};
use lizzie_core::workbench::Workbench;

struct NoEngines;
impl Launcher for NoEngines {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        Err(std::io::Error::other("no engines in panel tests"))
    }
}
fn storage(dir: &std::path::Path) -> Storage {
    Storage::in_dirs(&dir.join("config"), &dir.join("data"))
}
fn workbench(dir: &std::path::Path) -> Workbench {
    Workbench::new(storage(dir), Some("zh-CN"), Box::new(NoEngines), Vec::new())
}

#[test]
fn panel_toggles_are_projected_and_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    assert_eq!(wb.view(false).settings.panels, PanelSettings::default());
    for panel in [WorkbenchPanel::SubBoard, WorkbenchPanel::WinrateGraph, WorkbenchPanel::Comment,
        WorkbenchPanel::VariationGraph, WorkbenchPanel::ListPane, WorkbenchPanel::Captured,
        WorkbenchPanel::Status, WorkbenchPanel::AlwaysOnTop] {
        assert!(wb.toggle_panel(panel).unwrap());
    }
    let p = wb.view(false).settings.panels;
    assert!(!p.show_sub_board && !p.show_winrate_graph && !p.show_comment);
    assert!(!p.show_variation_graph && !p.show_list_pane && !p.show_captured && !p.show_status);
    assert!(p.always_on_top);
    drop(wb);
    assert_eq!(workbench(dir.path()).view(false).settings.panels, p);
}

#[test]
fn large_graph_and_subboard_use_java_interplay_and_restore_only_auto_hidden_subboard() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.toggle_panel(WorkbenchPanel::LargeWinrateGraph).unwrap();
    let p = wb.view(false).settings.panels;
    assert!(p.large_winrate_graph && p.hide_sub_board_from_large_winrate);
    assert!(!p.show_sub_board);
    wb.toggle_panel(WorkbenchPanel::LargeWinrateGraph).unwrap();
    let p = wb.view(false).settings.panels;
    assert!(p.show_sub_board && !p.hide_sub_board_from_large_winrate);
    wb.toggle_panel(WorkbenchPanel::SubBoard).unwrap();
    wb.toggle_panel(WorkbenchPanel::LargeWinrateGraph).unwrap();
    wb.toggle_panel(WorkbenchPanel::LargeWinrateGraph).unwrap();
    assert!(!wb.view(false).settings.panels.show_sub_board, "manually hidden stays hidden");
    wb.toggle_panel(WorkbenchPanel::LargeSubBoard).unwrap();
    let p = wb.view(false).settings.panels;
    assert!(p.show_sub_board && p.large_sub_board && !p.large_winrate_graph);
    wb.toggle_panel(WorkbenchPanel::LargeWinrateGraph).unwrap();
    assert!(!wb.view(false).settings.panels.large_sub_board);
    wb.toggle_panel(WorkbenchPanel::ListPane).unwrap();
    let p = wb.view(false).settings.panels;
    assert!(p.show_list_pane && !p.large_winrate_graph, "showing a hidden-by-mode list exits large graph");
}

#[test]
fn mode_entry_hides_comment_and_default_restores_panels_without_changing_status_preference() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.toggle_panel(WorkbenchPanel::Status).unwrap();
    wb.set_extra_mode(ExtraMode::DoubleEngine).unwrap();
    let s = wb.view(false).settings;
    assert!(s.comment_panel_auto_hidden);
    assert!(!s.panels.show_comment && !s.panels.show_sub_board && !s.panels.show_captured);
    assert!(!s.panels.show_variation_graph && !s.panels.show_winrate_graph);
    assert!(!wb.toggle_panel(WorkbenchPanel::Comment).unwrap());
    let mut draft = s.panels;
    draft.show_comment = true;
    wb.set_panel_settings(draft).unwrap();
    assert!(!wb.view(false).settings.panels.show_comment);
    drop(wb);
    let mut wb = workbench(dir.path());
    assert!(!wb.view(false).settings.panels.show_comment);
    wb.set_extra_mode(ExtraMode::Normal).unwrap();
    let s = wb.view(false).settings;
    assert!(!s.comment_panel_auto_hidden);
    assert!(s.panels.show_comment && s.panels.show_sub_board && s.panels.show_list_pane);
    assert!(!s.panels.show_status);
    wb.toggle_panel(WorkbenchPanel::SubBoard).unwrap();
    assert!(wb.set_extra_mode(ExtraMode::Normal).unwrap(), "default is also a panel restore operation");
    assert!(wb.view(false).settings.panels.show_sub_board);
}

#[test]
fn display_page_snapshot_and_top_toolbar_are_committed_together() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let mut draft = wb.view(false).settings.panels;
    draft.always_on_top = true;
    draft.show_winrate_in_title = false;
    draft.show_status = false;
    draft.show_sub_board = false;
    draft.show_top_toolbar = false;
    draft.show_double_menu = true;
    draft.independent_main_locked = true;
    draft.independent_sub_locked = true;
    draft.independent_main_top = true;
    draft.independent_sub_top = true;
    assert_eq!(wb.config().panels, PanelSettings::default(), "draft is not committed");
    wb.set_panel_settings(draft.clone()).unwrap();
    assert_eq!(wb.view(false).settings.panels, draft);
    drop(wb);
    assert_eq!(workbench(dir.path()).view(false).settings.panels, draft);
}

#[test]
fn size_shares_nudge_restore_and_reset_follow_the_persist_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let sizes = PanelSizes { leftover_left_share: Some(0.3), board_position_proportion: 2,
        comment_height_share: Some(0.6), variation_graph_share: Some(0.8) };
    wb.set_panel_sizes(sizes).unwrap();
    wb.set_bottom_toolbar(BottomToolbar::Detailed).unwrap();
    wb.nudge_board_position(1).unwrap();
    let expected = wb.view(false).settings.panel_sizes;
    assert_eq!(expected.leftover_left_share, Some(0.425));
    assert_eq!(expected.board_position_proportion, 3);
    drop(wb);
    let mut wb = workbench(dir.path());
    assert_eq!(wb.view(false).settings.panel_sizes, expected);
    assert_eq!(wb.view(false).settings.bottom_toolbar, BottomToolbar::Detailed);
    wb.restore_default_panel_sizes().unwrap();
    assert_eq!(wb.view(false).settings.panel_sizes, PanelSizes::default());
    wb.nudge_board_position(-1).unwrap();
    assert_eq!(wb.view(false).settings.panel_sizes.board_position_proportion, 3);
    assert_eq!(wb.view(false).settings.panel_sizes.leftover_left_share, None);
    assert!(!wb.nudge_board_position(-4).unwrap());
    wb.set_panel_sizes(sizes).unwrap();
    wb.reset_ui_position().unwrap();
    assert_eq!(wb.view(false).settings.panel_sizes, PanelSizes::default());
    assert_eq!(wb.view(false).settings.bottom_toolbar, BottomToolbar::Simple);
    assert!(!storage(dir.path()).persist_file.exists());
    wb.set_panel_sizes(sizes).unwrap();
    assert!(!storage(dir.path()).persist_file.exists(), "reset disables persist writes until restart");
    drop(wb);
    assert_eq!(workbench(dir.path()).view(false).settings.panel_sizes, PanelSizes::default());
}

#[test]
fn invalid_or_failed_size_commit_keeps_the_previous_projection() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    for invalid in [PanelSizes { leftover_left_share: Some(f64::NAN), ..PanelSizes::default() },
        PanelSizes { comment_height_share: Some(1.1), ..PanelSizes::default() },
        PanelSizes { board_position_proportion: 9, ..PanelSizes::default() }] {
        assert!(wb.set_panel_sizes(invalid).is_err());
        assert_eq!(wb.view(false).settings.panel_sizes, PanelSizes::default());
    }
    std::fs::create_dir_all(&storage(dir.path()).persist_file).unwrap();
    assert!(wb.set_panel_sizes(PanelSizes { comment_height_share: Some(0.4), ..PanelSizes::default() }).is_err());
    assert_eq!(wb.view(false).settings.panel_sizes, PanelSizes::default());
    assert!(wb.set_bottom_toolbar(BottomToolbar::Hidden).is_err());
    assert_eq!(wb.view(false).settings.bottom_toolbar, BottomToolbar::Simple);
}

#[test]
fn non_chinese_startup_restores_the_simple_toolbar_instead_of_obsolete_details() {
    // LizzieFrame.java:946-950; both Chinese bundles retain details, other languages do not.
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.set_bottom_toolbar(BottomToolbar::Detailed).unwrap();
    wb.set_language(lizzie_core::settings::Language::EnUs).unwrap();
    drop(wb);
    assert_eq!(workbench(dir.path()).view(false).settings.bottom_toolbar, BottomToolbar::Simple);
}

#[test]
fn fixed_modes_apply_java_panel_policies_and_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.set_classic_mode().unwrap();
    let p = wb.view(false).settings.panels;
    assert!(p.classic_mode && p.large_sub_board && p.show_sub_board && p.show_captured);
    assert!(!p.show_comment && !p.show_variation_graph);
    wb.set_extra_mode(ExtraMode::Thinking).unwrap();
    let s = wb.view(false).settings;
    assert!(s.panels.large_sub_board && s.panels.show_sub_board && s.panels.show_list_pane);
    assert!(!s.panels.show_comment && !s.comment_panel_auto_hidden);
    wb.toggle_panel(WorkbenchPanel::Comment).unwrap();
    assert!(wb.view(false).settings.panels.show_comment, "Thinking allows comment toggles");
    wb.set_extra_mode(ExtraMode::FourSub).unwrap();
    assert!(!wb.toggle_panel(WorkbenchPanel::Comment).unwrap());
    wb.toggle_panel(WorkbenchPanel::ListPane).unwrap();
    assert!(!wb.view(false).settings.panels.show_list_pane);
    wb.toggle_panel(WorkbenchPanel::ListPane).unwrap();
    assert_eq!(wb.view(false).settings.extra_mode, ExtraMode::FourSub);
    drop(wb);
    let mut wb = workbench(dir.path());
    let s = wb.view(false).settings;
    assert_eq!(s.extra_mode, ExtraMode::FourSub);
    assert!(s.comment_panel_auto_hidden && !s.panels.show_comment);
    wb.set_extra_mode(ExtraMode::Normal).unwrap();
    assert!(!wb.view(false).settings.panels.classic_mode);
}

#[test]
fn enabling_minimal_graph_comment_or_list_exits_without_restoring_other_panels() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    for panel in [WorkbenchPanel::WinrateGraph, WorkbenchPanel::Comment, WorkbenchPanel::ListPane] {
        wb.set_extra_mode(ExtraMode::Min).unwrap();
        let s = wb.view(false).settings;
        assert!(!s.panels.show_sub_board && !s.panels.show_comment && !s.panels.show_list_pane);
        wb.toggle_panel(panel).unwrap();
        let s = wb.view(false).settings;
        assert_eq!(s.extra_mode, ExtraMode::Normal);
        assert!(!s.panels.show_sub_board && !s.panels.show_captured && !s.panels.show_variation_graph);
    }
}

#[test]
fn independent_window_close_and_panel_toggle_have_distinct_java_policies() {
    use lizzie_core::settings::{IndependentBoard, WindowPlacement};
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.set_extra_mode(ExtraMode::FloatBoard).unwrap();
    let placement = WindowPlacement { x: -20, y: 50, width: 600, height: 650, maximized: false };
    wb.record_independent_board(IndependentBoard::Main, Some(placement)).unwrap();
    drop(wb);
    let mut wb = workbench(dir.path());
    let s = wb.view(false).settings;
    assert_eq!(s.independent_main_board, Some(placement));
    assert!(s.panels.showing_independent_main && s.panels.showing_independent_sub);
    wb.close_independent_board(IndependentBoard::Sub).unwrap();
    let s = wb.view(false).settings;
    assert_eq!(s.extra_mode, ExtraMode::FloatBoard);
    assert!(s.panels.show_sub_board && !s.panels.showing_independent_sub);
    wb.toggle_independent_board(IndependentBoard::Main).unwrap();
    let s = wb.view(false).settings;
    assert_eq!(s.extra_mode, ExtraMode::Normal);
    assert!(s.panels.showing_independent_main);
    wb.set_extra_mode(ExtraMode::FloatBoard).unwrap();
    wb.close_independent_board(IndependentBoard::Main).unwrap();
    let s = wb.view(false).settings;
    assert_eq!(s.extra_mode, ExtraMode::Normal);
    assert!(!s.panels.showing_independent_main && s.panels.show_sub_board && s.panels.show_comment);
}

#[test]
fn custom_slots_restore_only_java_snapshot_fields_after_restart() {
    use lizzie_core::settings::{IndependentBoard, WindowPlacement};
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.toggle_panel(WorkbenchPanel::Comment).unwrap();
    wb.toggle_panel(WorkbenchPanel::Status).unwrap();
    wb.set_panel_sizes(PanelSizes { board_position_proportion: 6, ..PanelSizes::default() }).unwrap();
    let bounds = WindowPlacement { x: 10, y: 20, width: 1400, height: 900, maximized: false };
    wb.persist_ui(Some(bounds), None, None).unwrap();
    wb.save_custom_layout(1, Some(bounds)).unwrap();
    wb.set_extra_mode(ExtraMode::FloatBoard).unwrap();
    wb.record_independent_board(IndependentBoard::Main, Some(bounds)).unwrap();
    wb.save_custom_layout(2, Some(bounds)).unwrap();
    wb.toggle_panel(WorkbenchPanel::AlwaysOnTop).unwrap();
    wb.set_bottom_toolbar(BottomToolbar::Hidden).unwrap();
    wb.restore_default_panel_sizes().unwrap();
    drop(wb);
    let mut wb = workbench(dir.path());
    wb.apply_custom_layout(1).unwrap();
    let s = wb.view(false).settings;
    assert_eq!(s.extra_mode, ExtraMode::Normal);
    assert!(!s.panels.show_comment && !s.panels.show_status);
    assert!(s.panels.always_on_top && s.panels.showing_independent_main, "snapshot does not close existing windows");
    assert_eq!(s.panel_sizes.board_position_proportion, 6);
    assert_eq!(s.bottom_toolbar, BottomToolbar::Hidden);
    wb.apply_custom_layout(2).unwrap();
    let s = wb.view(false).settings;
    assert_eq!(s.extra_mode, ExtraMode::FloatBoard);
    assert!(s.panels.show_comment && !s.panels.show_sub_board);
    assert_eq!(s.independent_main_board, Some(bounds));
    assert!(wb.save_custom_layout(0, None).is_err());
    assert!(wb.apply_custom_layout(3).is_err());
}

#[test]
fn failed_custom_layout_apply_keeps_both_committed_settings_and_projection() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.toggle_panel(WorkbenchPanel::Comment).unwrap();
    wb.save_custom_layout(1, None).unwrap();
    wb.set_extra_mode(ExtraMode::FourSub).unwrap();
    let before = wb.view(false).settings;
    let config_before = std::fs::read(storage(dir.path()).config_file).unwrap();
    // A failed persist write rolls the settings file back before publishing the projection.
    std::fs::create_dir_all(&storage(dir.path()).persist_file).unwrap();
    assert!(wb.apply_custom_layout(1).is_err());
    let after = wb.view(false).settings;
    assert_eq!(after.panels, before.panels);
    assert_eq!(after.extra_mode, before.extra_mode);
    assert_eq!(after.panel_sizes, before.panel_sizes);
    assert_eq!(std::fs::read(storage(dir.path()).config_file).unwrap(), config_before);
}

#[test]
fn classic_status_is_hidden_without_losing_normal_preference_and_toggle_leaves_classic() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.set_classic_mode().unwrap();
    wb.set_extra_mode(ExtraMode::Normal).unwrap();
    assert!(wb.view(false).settings.panels.show_status);
    wb.set_classic_mode().unwrap();
    wb.toggle_panel(WorkbenchPanel::Status).unwrap();
    let p = wb.view(false).settings.panels;
    assert!(p.show_status && !p.classic_mode);
}

#[test]
fn every_fixed_layout_selection_and_panel_preference_is_reloaded() {
    let dir = tempfile::tempdir().unwrap();
    for mode in [ExtraMode::Normal, ExtraMode::Thinking, ExtraMode::FourSub,
        ExtraMode::DoubleEngine, ExtraMode::Min, ExtraMode::FloatBoard] {
        let mut wb = workbench(dir.path());
        wb.set_extra_mode(mode).unwrap();
        wb.toggle_panel(WorkbenchPanel::AlwaysOnTop).unwrap();
        let panels = wb.view(false).settings.panels;
        drop(wb);
        let s = workbench(dir.path()).view(false).settings;
        assert_eq!(s.extra_mode, mode);
        assert_eq!(s.panels, panels);
    }
}

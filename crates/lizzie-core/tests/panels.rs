//! Java's workbench presentation through the same operations used by menus and shortcuts.
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::settings::{BigWinrateFit, BigWinrateSettings, BigWinrateStyle, BottomToolbar, ExtraMode, PanelSettings, PanelSizes, Storage, WorkbenchPanel};
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
/// SetCustomMode(slot) 确定 with the dialog left as it opened: the current layout is the snapshot.
fn save_slot(wb: &mut Workbench, slot: u8, window: Option<lizzie_core::settings::WindowPlacement>) -> Result<bool, lizzie_core::api::ApiError> {
    let s = wb.view(false).settings;
    wb.commit_layout(s.extra_mode, s.panels, s.panel_sizes.board_position_proportion, Some((slot, window)))
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
    wb.set_bottom_toolbar(BottomToolbar::Hidden).unwrap();
    wb.nudge_board_position(1).unwrap();
    let expected = wb.view(false).settings.panel_sizes;
    assert_eq!(expected.leftover_left_share, Some(0.425));
    assert_eq!(expected.board_position_proportion, 3);
    drop(wb);
    let mut wb = workbench(dir.path());
    assert_eq!(wb.view(false).settings.panel_sizes, expected);
    assert_eq!(wb.view(false).settings.bottom_toolbar, BottomToolbar::Hidden);
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
fn bottom_buttons_survive_restart() {
    use lizzie_core::settings::BottomButtonSettings;
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    assert_eq!(wb.view(false).settings.bottom_buttons, BottomButtonSettings::default());
    let buttons = BottomButtonSettings { refresh: true, try_play: true, bad_moves: Some(false), ..Default::default() };
    assert!(wb.set_bottom_buttons(buttons).unwrap());
    drop(wb);
    assert_eq!(workbench(dir.path()).view(false).settings.bottom_buttons, buttons);
}

/// The top toolbar's 候选点 field choice (off by default), its 激进度 / 广度 choices (shown by
/// default; the same pair as the 引擎参数 window's 在工具栏中控制) and the 候选点上限 the field
/// edits are read back after a restart; a value that cannot be read gives that setting its
/// default and keeps the rest of the file.
#[test]
fn toolbar_candidate_limit_and_parameter_choices_survive_restart() {
    use lizzie_core::engine::runtime::KataParam;
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let v = wb.view(false).settings;
    assert!(!v.top_toolbar.show_suggestion_limit);
    assert!(v.engine.kata_params.show_pda && v.engine.kata_params.show_wrn);
    assert_eq!(v.engine.analysis.limit_max_suggestion, 10);
    assert!(wb.set_top_toolbar_settings(lizzie_core::settings::TopToolbarSettings { show_suggestion_limit: true, ..v.top_toolbar }).unwrap());
    // Neither choice needs an engine; choosing the saved value again changes nothing.
    assert!(wb.set_kata_param_shown(KataParam::Pda, false).unwrap());
    assert!(wb.set_kata_param_shown(KataParam::Wrn, false).unwrap());
    assert!(!wb.set_kata_param_shown(KataParam::Wrn, false).unwrap());
    assert!(wb.set_analysis_settings(lizzie_core::engine::profile::AnalysisSettings { limit_max_suggestion: 3, ..v.engine.analysis }).unwrap());
    drop(wb);
    let v = workbench(dir.path()).view(false).settings;
    assert!(v.top_toolbar.show_suggestion_limit);
    assert_eq!((v.engine.kata_params.show_pda, v.engine.kata_params.show_wrn), (false, false));
    assert_eq!(v.engine.analysis.limit_max_suggestion, 3);

    let file = storage(dir.path()).config_file;
    let mut stored: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    stored["topToolbar"]["showSuggestionLimit"] = "yes".into();
    stored["engine"]["kataParams"]["showPda"] = 1.into();
    stored["engine"]["analysis"]["limitMaxSuggestion"] = (-3).into();
    std::fs::write(&file, serde_json::to_vec(&stored).unwrap()).unwrap();
    let v = workbench(dir.path()).view(false).settings;
    assert!(!v.top_toolbar.show_suggestion_limit);
    assert!(v.engine.kata_params.show_pda);
    assert_eq!(v.engine.analysis.limit_max_suggestion, 10);
    assert!(!v.engine.kata_params.show_wrn, "the readable choices beside them stay");
    assert!(!file.with_file_name("config.json.unreadable-backup").exists());

    for limit in [serde_json::json!(2.5), serde_json::json!("3"), serde_json::json!(5_000_000_000u64)] {
        let mut stored: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        stored["engine"]["analysis"]["limitMaxSuggestion"] = limit.clone();
        std::fs::write(&file, serde_json::to_vec(&stored).unwrap()).unwrap();
        assert_eq!(workbench(dir.path()).view(false).settings.engine.analysis.limit_max_suggestion, 10, "{limit}");
    }
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
    save_slot(&mut wb, 1, Some(bounds)).unwrap();
    wb.set_extra_mode(ExtraMode::FloatBoard).unwrap();
    wb.record_independent_board(IndependentBoard::Main, Some(bounds)).unwrap();
    save_slot(&mut wb, 2, Some(bounds)).unwrap();
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
    assert!(save_slot(&mut wb, 0, None).is_err());
    assert!(wb.apply_custom_layout(3).is_err());
}

#[test]
fn failed_custom_layout_apply_keeps_both_committed_settings_and_projection() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.toggle_panel(WorkbenchPanel::Comment).unwrap();
    save_slot(&mut wb, 1, None).unwrap();
    wb.set_extra_mode(ExtraMode::FourSub).unwrap();
    let before = wb.view(false).settings;
    let config_before = std::fs::read(storage(dir.path()).config_file).unwrap();
    // A failed persist write rolls the settings file back before publishing the projection.
    let persist = storage(dir.path()).persist_file;
    let _ = std::fs::remove_file(&persist);
    std::fs::create_dir_all(&persist).unwrap();
    assert!(wb.apply_custom_layout(1).is_err());
    let after = wb.view(false).settings;
    assert_eq!(after.panels, before.panels);
    assert_eq!(after.extra_mode, before.extra_mode);
    assert_eq!(after.panel_sizes, before.panel_sizes);
    assert_eq!(std::fs::read(storage(dir.path()).config_file).unwrap(), config_before);
}

#[test]
fn applying_a_custom_layout_moves_a_dragged_board_share_to_the_saved_position() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.set_panel_sizes(PanelSizes { board_position_proportion: 2, ..PanelSizes::default() }).unwrap();
    save_slot(&mut wb, 1, None).unwrap();
    // A separator drag leaves a fine share far to the right of position 2.
    wb.set_panel_sizes(PanelSizes { board_position_proportion: 7, leftover_left_share: Some(0.9), ..PanelSizes::default() }).unwrap();
    wb.apply_custom_layout(1).unwrap();
    let sizes = wb.view(false).settings.panel_sizes;
    assert_eq!(sizes.board_position_proportion, 2);
    assert_eq!(sizes.leftover_left_share, Some(0.25));
    drop(wb);
    assert_eq!(workbench(dir.path()).view(false).settings.panel_sizes.leftover_left_share, Some(0.25));
}

#[test]
fn layout_dialog_commit_is_all_or_nothing_and_keeps_an_untouched_board_share() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.set_panel_sizes(PanelSizes { board_position_proportion: 4, leftover_left_share: Some(0.6), ..PanelSizes::default() }).unwrap();
    let before = wb.view(false).settings;
    let config_before = std::fs::read(storage(dir.path()).config_file).ok();
    let mut panels = before.panels.clone();
    panels.show_variation_graph = false;
    // The position file cannot be written: neither the panels, the position nor the slot change.
    let persist = storage(dir.path()).persist_file;
    let saved = std::fs::read(&persist).unwrap();
    std::fs::remove_file(&persist).unwrap();
    std::fs::create_dir_all(&persist).unwrap();
    assert!(wb.commit_layout(ExtraMode::Normal, panels.clone(), 6, Some((1, None))).is_err());
    let after = wb.view(false).settings;
    assert_eq!((after.panels, after.panel_sizes, after.custom_layouts[0].is_none()), (before.panels.clone(), before.panel_sizes, true));
    assert_eq!(std::fs::read(storage(dir.path()).config_file).ok(), config_before);
    std::fs::remove_dir(&persist).unwrap();
    std::fs::write(&persist, saved).unwrap();
    // The retry saves all of it; a moved slider moves the fine share with it.
    assert!(wb.commit_layout(ExtraMode::Normal, panels.clone(), 6, Some((1, None))).unwrap());
    let s = wb.view(false).settings;
    assert!(!s.panels.show_variation_graph);
    assert_eq!((s.panel_sizes.board_position_proportion, s.panel_sizes.leftover_left_share), (6, Some(0.75)));
    assert_eq!(s.custom_layouts[0].as_ref().unwrap().board_position_proportion, 6);
    // An untouched slider keeps the dragged share.
    wb.set_panel_sizes(PanelSizes { board_position_proportion: 6, leftover_left_share: Some(0.7), ..PanelSizes::default() }).unwrap();
    wb.commit_layout(ExtraMode::Normal, panels, 6, None).unwrap();
    assert_eq!(wb.view(false).settings.panel_sizes.leftover_left_share, Some(0.7));
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

/// 黑白胜率大字's look and 窗口置顶 are kept as soon as they are chosen and come back after a
/// restart with the bounds it closed at; 重置界面位置 leaves them. Bounds too small to use, and
/// stored values this version cannot read, come back as none or the default without stopping the
/// rest of the settings from loading.
#[test]
fn big_winrate_settings_survive_restart_and_unreadable_values_fall_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    assert_eq!(wb.view(false).settings.big_winrate, BigWinrateSettings::default());
    assert_eq!(BigWinrateSettings::default().style, BigWinrateStyle::Plain);
    assert!(wb.set_big_winrate(BigWinrateStyle::Led, true).unwrap());
    assert!(!wb.set_big_winrate(BigWinrateStyle::Led, true).unwrap(), "unchanged");
    wb.record_big_winrate_bounds([40, 50, 600, 300], [0, 37]).unwrap();
    wb.reset_ui_position().unwrap();
    drop(wb);
    let mut wb = workbench(dir.path());
    let kept = BigWinrateSettings { style: BigWinrateStyle::Led, on_top: true, bounds: Some([40, 50, 600, 300]), frame: Some([0, 37]) };
    assert_eq!(wb.view(false).settings.big_winrate, kept);
    wb.record_big_winrate_bounds([40, 50, 119, 300], [0, 37]).unwrap();
    assert_eq!(wb.view(false).settings.big_winrate.bounds, None, "narrower than 120");
    wb.record_big_winrate_bounds([40, 50, 120, 60], [0, 37]).unwrap();
    assert_eq!(wb.view(false).settings.big_winrate.bounds, Some([40, 50, 120, 60]));
    wb.toggle_panel(WorkbenchPanel::AlwaysOnTop).unwrap();
    drop(wb);

    let file = dir.path().join("config").join("config.json");
    let mut config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    config["bigWinrate"] = serde_json::json!({ "style": "Neon", "onTop": "yes", "bounds": [1, 2, 300, 59] });
    std::fs::write(&file, config.to_string()).unwrap();
    let s = workbench(dir.path()).view(false).settings;
    assert_eq!(s.big_winrate, BigWinrateSettings::default());
    assert!(s.panels.always_on_top, "the rest still loads");
    config["bigWinrate"] = serde_json::json!("damaged");
    std::fs::write(&file, config.to_string()).unwrap();
    let s = workbench(dir.path()).view(false).settings;
    assert_eq!(s.big_winrate, BigWinrateSettings::default());
    assert!(s.panels.always_on_top, "the rest still loads");
}

/// Closing and opening 黑白胜率大字 again, in one session or after a restart, keeps the whole
/// window's size: the window opens with the kept bounds less the kept frame. Modelled on a window
/// system that adds the frame around the content it is given: none, a server-side title bar, and
/// GTK's own header bar and shadow on Wayland, which is told only once the window is shown.
#[test]
fn big_winrate_size_does_not_drift_over_reopens_and_restarts() {
    for frame in [[0, 0], [2, 25], [52, 89]] {
        let dir = tempfile::tempdir().unwrap();
        let mut wb = workbench(dir.path());
        // First open: no frame known yet, the content is Java's whole size.
        let mut content = [520, 260];
        let mut sizes = vec![];
        // The first open, three reopens in the session, then three restarts.
        for round in 0..7 {
            if round >= 4 {
                drop(wb);
                wb = workbench(dir.path());
            }
            let saved = wb.view(false).settings.big_winrate;
            if let (Some([_, _, w, h]), Some(kept)) = (saved.bounds, saved.frame) {
                content = BigWinrateSettings::content_for([w, h], kept);
            }
            let outer = [content[0] + frame[0], content[1] + frame[1]];
            assert_eq!(BigWinrateSettings::frame_of(outer, content), frame);
            wb.record_big_winrate_bounds([10, 20, outer[0], outer[1]], BigWinrateSettings::frame_of(outer, content)).unwrap();
            sizes.push(outer);
        }
        assert!(sizes.iter().all(|s| *s == sizes[0]), "frame {frame:?}: {sizes:?}");
        assert_eq!(sizes[0], [520 + frame[0], 260 + frame[1]]);
    }
    assert_eq!(BigWinrateSettings::content_for([130, 70], [52, 89]), [120, 60], "never under 120 × 60");
    assert_eq!(BigWinrateSettings::content_for([600, 300], [-5, -5]), [600, 300], "a frame is never negative");
    assert_eq!(BigWinrateSettings::frame_of([600, 300], [610, 300]), [0, 0]);
}

/// One session of a window system that frames windows with `frame`, as the shell opens
/// 黑白胜率大字 on Linux: from the kept bounds less the kept frame, refitted as the window system
/// reports the window. Its reports come first before the window manager has framed it (as on
/// X11), and a report of the content asked for before may still come after a new one was asked
/// for. The user may then give it `resize` (content). It closes and its bounds are kept. Returns
/// the whole size it was shown at before any resize.
fn big_winrate_session(wb: &mut Workbench, frame: [i32; 2], resize: Option<[i32; 2]>) -> [i32; 2] {
    let whole = |c: [i32; 2]| [c[0] + frame[0], c[1] + frame[1]];
    let saved = wb.view(false).settings.big_winrate;
    let [_, _, w, h] = saved.bounds.unwrap();
    let mut fit = BigWinrateFit::new([w, h], saved.frame.unwrap());
    let mut content = fit.content();
    let mut reports = std::collections::VecDeque::from([(content, content), (whole(content), content)]);
    while let Some((outer, reported)) = reports.pop_front() {
        if let Some(asked) = fit.measured(outer, reported) {
            reports.extend([(whole(content), content), (whole(asked), asked)]);
            content = asked;
        }
    }
    let shown = whole(content);
    if let Some(resized) = resize {
        content = resized;
        assert_eq!(fit.measured(whole(content), content), None, "a size the user gave is left alone");
    }
    let outer = whole(content);
    let [w, h] = fit.kept_outer(outer, content);
    wb.record_big_winrate_bounds([490, 320, w, h], BigWinrateSettings::frame_of(outer, content)).unwrap();
    shown
}

/// 黑白胜率大字 keeps the whole size it was closed at when the window manager's decorations
/// change between sessions (a title bar on, then off, then a different one), and the size kept
/// is not rewritten by the change; a size the user gives it is kept and comes back whole under
/// other decorations.
#[test]
fn big_winrate_whole_size_survives_a_change_of_decorations() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    // Closed at 520 × 260 whole under a server-side title bar.
    wb.record_big_winrate_bounds([490, 320, 520, 260], [2, 25]).unwrap();
    for frame in [[0, 0], [2, 25], [52, 89], [0, 0], [2, 25]] {
        drop(wb);
        wb = workbench(dir.path());
        assert_eq!(big_winrate_session(&mut wb, frame, None), [520, 260], "shown whole under {frame:?}");
        let saved = wb.view(false).settings.big_winrate;
        assert_eq!(saved.bounds, Some([490, 320, 520, 260]), "kept under {frame:?}");
        assert_eq!(saved.frame, Some(frame));
    }
    // The user makes it 600 × 300 whole without decorations; with a title bar it is 600 × 300 whole.
    drop(wb);
    wb = workbench(dir.path());
    big_winrate_session(&mut wb, [0, 0], Some([600, 300]));
    assert_eq!(wb.view(false).settings.big_winrate.bounds, Some([490, 320, 600, 300]));
    drop(wb);
    wb = workbench(dir.path());
    assert_eq!(big_winrate_session(&mut wb, [2, 25], None), [600, 300]);
    assert_eq!(wb.view(false).settings.big_winrate.bounds, Some([490, 320, 600, 300]));
}

/// A window system whose reports keep disagreeing about the frame gets the content set again at
/// most twice; once the window has another size than the one asked for, it is left alone.
#[test]
fn big_winrate_refits_a_bounded_number_of_times_and_never_after_another_size() {
    let mut fit = BigWinrateFit::new([520, 260], [2, 25]);
    assert_eq!(fit.content(), [518, 235]);
    assert_eq!(fit.measured([518, 235], [518, 235]), Some([520, 260]));
    assert_eq!(fit.measured([520, 260], [518, 235]), None, "the content asked for before is still reported");
    assert_eq!(fit.measured([522, 285], [520, 260]), Some([518, 235]));
    assert_eq!(fit.measured([518, 235], [518, 235]), None, "no third refit");
    assert_eq!(fit.kept_outer([518, 235], [518, 235]), [520, 260], "still the size asked for");

    let mut fit = BigWinrateFit::new([520, 260], [0, 0]);
    assert_eq!(fit.measured([530, 290], [528, 265]), None, "another size: left alone");
    assert_eq!(fit.measured([522, 285], [520, 260]), None, "and stays left alone");
    assert_eq!(fit.kept_outer([530, 290], [528, 265]), [530, 290]);

    let mut fit = BigWinrateFit::new([520, 260], [0, 0]);
    fit.settle();
    assert_eq!(fit.measured([522, 285], [520, 260]), None, "settled: no refit");
    assert_eq!(fit.kept_outer([522, 285], [520, 260]), [520, 260], "the size asked for keeps the kept whole size");
}

//! Appearance operations through the same committed settings/projection seam as the UI.
use lizzie_core::appearance::{self, AppearanceSettings, BoardStyle, ThemeSettings};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::settings::Storage;
use lizzie_core::workbench::Workbench;

struct NoEngine;
impl Launcher for NoEngine {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> { Err(std::io::Error::other("not started")) }
}
fn storage(root:&std::path::Path)->Storage { Storage::in_dirs(&root.join("config"),&root.join("data")) }
fn workbench(root:&std::path::Path)->Workbench { Workbench::new(storage(root),Some("zh-CN"),Box::new(NoEngine),Vec::new()) }

#[test]
fn default_edits_apply_and_survive_restart_without_mutating_a_draft() {
    let dir=tempfile::tempdir().unwrap(); let mut wb=workbench(dir.path());
    let before=wb.view(true).settings;
    assert_eq!(before.theme,ThemeSettings::default());
    let mut draft=before.theme.clone(); draft.use_pure_board=true; draft.pure_board_color=vec![20,40,60];
    draft.font_name="Example Font".into(); draft.comment_font_size=19; draft.winrate_stroke_width=2.8;
    assert_eq!(wb.view(false).settings.theme,before.theme);
    let mut settings=before.appearance; settings.board_style=BoardStyle::ChineseClassic; settings.apple_style=true;
    wb.set_appearance_settings(settings,draft.clone()).unwrap();
    let v=wb.view(false).settings;
    assert_eq!(v.theme,draft); assert!(v.appearance.morandi_colors); assert_eq!(v.appearance.board_style,BoardStyle::ChineseClassic);
    let mut reopened=workbench(dir.path()); assert_eq!(reopened.view(true).settings.theme,draft);
}
#[test]
fn frame_size_is_saved_but_the_session_keeps_its_original_size() {
    let dir=tempfile::tempdir().unwrap(); let mut wb=workbench(dir.path());
    wb.set_frame_font_size(16).unwrap();
    let s=wb.view(false).settings; assert_eq!(s.appearance.frame_font_size,16); assert_eq!(s.active_frame_font_size,12);
    assert!(wb.set_frame_font_size(11).is_err()); assert!(wb.set_frame_font_size(21).is_err());
    assert_eq!(workbench(dir.path()).view(true).settings.active_frame_font_size,16);
}
#[test]
fn named_theme_edits_use_the_selected_theme_file_and_leave_default_values_alone() {
    let dir=tempfile::tempdir().unwrap(); let mut wb=workbench(dir.path());
    let mut settings=AppearanceSettings::default(); settings.theme="sabaki".into();
    let mut theme=appearance::load_theme(wb.storage(),"sabaki",&ThemeSettings::default());
    theme.comment_font_color=vec![10,20,30,240]; theme.score_lead_stroke_width=3.2;
    wb.set_appearance_settings(settings,theme.clone()).unwrap();
    assert_eq!(wb.config().appearance_defaults,ThemeSettings::default());
    let file=appearance::theme_file(wb.storage(),"sabaki").unwrap();
    let raw:serde_json::Value=serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
    assert_eq!(raw["comment-font-color"],serde_json::json!([10,20,30,240]));
    assert_eq!(workbench(dir.path()).view(true).settings.theme,theme);
}
#[test]
fn a_failed_theme_or_config_write_does_not_publish_new_settings() {
    let dir=tempfile::tempdir().unwrap(); let mut wb=workbench(dir.path()); let before=wb.view(true).settings;
    std::fs::create_dir_all(&storage(dir.path()).config_file).unwrap();
    let mut next=before.appearance.clone(); next.theme="sabaki".into();
    let mut theme=before.theme.clone(); theme.use_pure_stone=true;
    assert!(wb.set_appearance_settings(next,theme).is_err());
    assert_eq!(wb.view(false).settings,before);
    assert!(!appearance::theme_file(wb.storage(),"sabaki").unwrap().exists());
}
#[test]
fn new_themes_use_the_java_template_and_deletion_removes_only_the_chosen_theme() {
    let dir=tempfile::tempdir().unwrap(); let s=storage(dir.path()); let mut wb=workbench(dir.path());
    wb.add_theme("Personal").unwrap();
    let theme=appearance::load_theme(&s,"Personal",&ThemeSettings::default());
    assert_eq!(theme.black_stone_image,"black.png");
    assert!(wb.add_theme("personal").is_err());
    assert!(wb.add_theme("../outside").is_err());
    assert!(wb.delete_theme("default").is_err());
    let settings=AppearanceSettings{theme:"Personal".into(),..Default::default()};
    assert!(appearance::images(&s,&settings,&theme).board.starts_with("data:image/"));
    wb.delete_theme("Personal").unwrap();
    assert!(!appearance::catalog(&s,&ThemeSettings::default()).iter().any(|t|t.name=="Personal"));
    assert!(appearance::catalog(&s,&ThemeSettings::default()).iter().any(|t|t.name=="sabaki"));
    wb.delete_theme("sabaki").unwrap();
    assert!(!appearance::catalog(&s,&ThemeSettings::default()).iter().any(|t|t.name=="sabaki"));
    assert_eq!(appearance::load_theme(&s,"sabaki",&ThemeSettings::default()),ThemeSettings::default());
}
#[test]
fn custom_images_are_owned_copies_and_clear_restores_the_default() {
    let dir=tempfile::tempdir().unwrap(); let mut wb=workbench(dir.path());
    let external=dir.path().join("selected.png");
    std::fs::write(&external,include_bytes!("../../../ui/public/assets/board.png")).unwrap();
    let mut settings=AppearanceSettings::default(); settings.custom_board_image=external.to_string_lossy().into_owned();
    wb.set_appearance_settings(settings,ThemeSettings::default()).unwrap();
    std::fs::remove_file(external).unwrap();
    let s=wb.view(false).settings;
    assert!(std::path::Path::new(&s.appearance.custom_board_image).starts_with(appearance::themes_dir(wb.storage())));
    assert!(appearance::images(wb.storage(),&s.appearance,&s.theme).board.starts_with("data:image/png;base64,"));
    assert!(appearance::images(wb.storage(),&s.appearance,&s.theme).custom_board);
    wb.set_appearance_settings(AppearanceSettings::default(),s.theme).unwrap();
    let mut wb=workbench(dir.path()); let s=wb.view(true).settings;
    assert_eq!(appearance::images(wb.storage(),&s.appearance,&s.theme).board,"/assets/board.png");
}
#[test]
fn malformed_legacy_blunder_rules_fall_back_together_without_losing_other_theme_fields() {
    let base=ThemeSettings::default();
    let raw=serde_json::json!({"comment-font-size":22,"blunder-winrate-thresholds":[1,1],"blunder-node-colors":[[0,0,0],[255,255,255]],"best-move-color":[999,0,0]});
    let theme=appearance::merge_java(&base,raw.as_object().unwrap());
    assert_eq!(theme.comment_font_size,22); assert_eq!(theme.blunder_winrate_thresholds,base.blunder_winrate_thresholds);
    assert_eq!(theme.best_move_color,base.best_move_color);
}

#[test]
fn failed_image_settings_commit_removes_its_uncommitted_copies() {
    let dir=tempfile::tempdir().unwrap(); let mut wb=workbench(dir.path());
    let external=dir.path().join("selected.png");
    std::fs::write(&external,include_bytes!("../../../ui/public/assets/board.png")).unwrap();
    std::fs::create_dir_all(&storage(dir.path()).config_file).unwrap();
    let mut settings=AppearanceSettings::default(); settings.custom_board_image=external.to_string_lossy().into_owned();
    assert!(wb.set_appearance_settings(settings,ThemeSettings::default()).is_err());
    let images=appearance::themes_dir(wb.storage()).join(".images");
    assert_eq!(std::fs::read_dir(images).unwrap().count(),0);
    assert!(external.is_file());
}

#[test]
fn variation_tree_colors_cover_branches_and_exempt_the_real_best_move() {
    let dir=tempfile::tempdir().unwrap(); let mut wb=workbench(dir.path());
    // Root's E5 is best; both E5 and G3 lose 10% for Black (root LZOP holds the opponent's
    // win rate, so 40.0 is Black's 60%). Only E5 gets 一选免罚.
    let record="(;SZ[9]KM[7.5]LZOP[b11 40.0 100 2 0.5 0 rootVisits=100\nmove E5 visits 100 winrate 6000 scoreMean 2 pv E5]\
        (;B[ee]LZ[b11 50.0 100 -1 0.5 0 rootVisits=100\nmove C3 visits 100 winrate 5000 scoreMean -1 pv C3])\
        (;B[gg]LZ[b11 50.0 100 -1 0.5 0 rootVisits=100\nmove C3 visits 100 winrate 5000 scoreMean -1 pv C3]))";
    let ticket=wb.begin_open(lizzie_core::workbench::OpenOrigin::Clipboard);
    wb.complete_open(ticket,Workbench::read_clipboard_record(record,false));
    let rec=&wb.document().record;
    let best=rec.node(lizzie_core::record::ROOT).children[0]; let other=rec.node(lizzie_core::record::ROOT).children[1];
    let colors=wb.view(true).theme_nodes;
    assert_eq!(colors.iter().find(|v|v.node==best).unwrap().color,vec![0,220,0]);
    assert_eq!(colors.iter().find(|v|v.node==other).unwrap().color,vec![200,140,50]);
    let mut theme=ThemeSettings::default(); theme.blunder_node_colors[5]=vec![10,20,30];
    wb.set_appearance_settings(AppearanceSettings::default(),theme).unwrap();
    assert_eq!(wb.view(false).theme_nodes.iter().find(|v|v.node==best).unwrap().color,vec![10,20,30]);
}

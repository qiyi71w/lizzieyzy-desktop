//! Appearance operations through the same committed settings/projection seam as the UI.
use lizzie_core::appearance::{self, AppearanceMode, AppearanceSettings, BoardStyle, ThemeSettings};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::settings::Storage;
use lizzie_core::workbench::{Workbench, SETTINGS_SAVE_FAILED};

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
    assert_eq!(before.theme,ThemeSettings::default()); assert_eq!(before.appearance.mode,AppearanceMode::Light);
    let mut draft=before.theme.clone(); draft.use_pure_board=true; draft.pure_board_color=vec![20,40,60];
    draft.font_name="Example Font".into(); draft.comment_font_size=19; draft.winrate_stroke_width=2.8;
    assert_eq!(wb.view(false).settings.theme,before.theme);
    let mut settings=before.appearance; settings.board_style=BoardStyle::ChineseClassic; settings.mode=AppearanceMode::System;
    wb.set_appearance_settings(settings.clone(),draft.clone()).unwrap();
    let v=wb.view(false).settings;
    assert_eq!(v.theme,draft); assert_eq!(v.appearance.mode,AppearanceMode::System); assert_eq!(v.appearance.board_style,BoardStyle::ChineseClassic);
    let mut reopened=workbench(dir.path()); let v=reopened.view(true).settings;
    assert_eq!(v.theme,draft); assert_eq!(v.appearance.mode,AppearanceMode::System);
    settings.mode=AppearanceMode::Dark;
    reopened.set_appearance_settings(settings,draft).unwrap();
    assert_eq!(workbench(dir.path()).view(true).settings.appearance.mode,AppearanceMode::Dark);
}
#[test]
fn settings_written_with_the_former_interface_styles_still_load() {
    let dir=tempfile::tempdir().unwrap(); let file=storage(dir.path()).config_file;
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file,r#"{"appearance":{"boardStyle":"ChineseClassic","appleStyle":true,"morandiColors":true,"frameFontSize":16}}"#).unwrap();
    let a=workbench(dir.path()).view(true).settings.appearance;
    assert_eq!((a.board_style,a.mode,a.frame_font_size),(BoardStyle::ChineseClassic,AppearanceMode::Light,16));
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
    let settings=AppearanceSettings { theme:"sabaki".into(), ..Default::default() };
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
fn saving_a_named_theme_keeps_the_theme_file_keys_the_page_does_not_edit() {
    let dir=tempfile::tempdir().unwrap(); let mut wb=workbench(dir.path());
    wb.add_theme("Personal").unwrap();
    let file=appearance::theme_file(wb.storage(),"Personal").unwrap();
    let mut raw:serde_json::Map<String,serde_json::Value>=serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    raw.insert("node-color-mode".into(),serde_json::json!(1));
    raw.insert("future-theme-key".into(),serde_json::json!({"nested":[1,"two"]}));
    std::fs::write(&file,serde_json::to_vec(&raw).unwrap()).unwrap();
    let mut theme=appearance::load_theme(wb.storage(),"Personal",&ThemeSettings::default());
    theme.shadow_size=42; theme.background_image=String::new();
    wb.set_appearance_settings(AppearanceSettings { theme:"Personal".into(), ..Default::default() },theme.clone()).unwrap();
    let saved:serde_json::Value=serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(saved["node-color-mode"],serde_json::json!(1));
    assert_eq!(saved["future-theme-key"],serde_json::json!({"nested":[1,"two"]}));
    assert_eq!(saved["shadow-size"],serde_json::json!(42));
    // Theme.save leaves out blank strings.
    assert!(saved.get("background-image").is_none());
    assert_eq!(wb.view(false).settings.theme,theme);
    // A bundled theme without a user file keeps the bundled file's other keys.
    let theme=appearance::load_theme(wb.storage(),"Fast",&ThemeSettings::default());
    wb.set_appearance_settings(AppearanceSettings { theme:"Fast".into(), ..Default::default() },ThemeSettings { shadow_size:43, ..theme }).unwrap();
    let saved:serde_json::Value=serde_json::from_slice(&std::fs::read(appearance::theme_file(wb.storage(),"Fast").unwrap()).unwrap()).unwrap();
    assert_eq!(saved["var-panel-color"],serde_json::json!([80,80,80]));
}
#[test]
fn saving_a_named_theme_writes_only_the_keys_the_page_edits() {
    let dir=tempfile::tempdir().unwrap(); let mut wb=workbench(dir.path());
    wb.add_theme("Personal").unwrap();
    let file=appearance::theme_file(wb.storage(),"Personal").unwrap();
    let raw:serde_json::Value=serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert!(raw.get("glass-corner-radius").is_none());
    let theme=appearance::load_theme(wb.storage(),"Personal",&ThemeSettings::default());
    wb.set_appearance_settings(AppearanceSettings { theme:"Personal".into(), ..Default::default() },ThemeSettings { shadow_size:42, ..theme }).unwrap();
    let saved:serde_json::Value=serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(saved["shadow-size"],serde_json::json!(42));
    // Keys the page does not edit stay absent, so they keep following the ui values.
    assert!(saved.get("glass-corner-radius").is_none(), "{saved}");
    assert!(saved.get("glass-accent-color").is_none(), "{saved}");
}
/// Every value the 主题外观 page edits (ConfigDialog2.java:6204-6285, the 界面颜色 and the 棋盘 /
/// 棋子 choices) is written by a save and read back the same.
#[test]
fn every_value_the_theme_page_edits_survives_a_save_and_a_reload() {
    let dir=tempfile::tempdir().unwrap(); let mut wb=workbench(dir.path());
    wb.add_theme("Personal").unwrap();
    let theme=appearance::load_theme(wb.storage(),"Personal",&ThemeSettings::default());
    let edited=ThemeSettings {
        use_pure_board:!theme.use_pure_board, use_pure_background:!theme.use_pure_background, use_pure_stone:!theme.use_pure_stone,
        pure_board_color:vec![1,2,3], pure_background_color:vec![4,5,6],
        show_stone_shadow:!theme.show_stone_shadow, shadow_size:77, stone_indicator_type:(theme.stone_indicator_type+1)%4,
        winrate_stroke_width:2.5, score_lead_stroke_width:3.5, minimum_blunder_bar_width:5,
        comment_font_size:17, background_filter:9,
        winrate_line_color:vec![7,8,9], winrate_miss_line_color:vec![10,11,12], scoremean_line_color:vec![13,14,15],
        blunder_bar_color:vec![16,17,18,19], best_move_color:vec![20,21,22],
        comment_background_color:vec![23,24,25,26], comment_font_color:vec![27,28,29], comment_node_color:vec![30,31,32],
        show_comment_node_color:!theme.show_comment_node_color, use_scorediff_in_variation_tree:!theme.use_scorediff_in_variation_tree,
        scorediff_in_variation_tree_factor:0.25,
        font_name:"Serif".into(), ui_font_name:"Sans".into(), winrate_font_name:"Mono".into(),
        blunder_winrate_thresholds:vec![-20.0,-5.0,0.0], blunder_node_colors:vec![vec![1,1,1],vec![2,2,2],vec![3,3,3]],
        ui_chrome_color:vec![33,34,35], ui_control_color:vec![36,37,38], ui_accent_color:vec![39,40,41], ui_text_color:vec![42,43,44],
        board_image:"/themes/sabaki/board.png".into(), black_stone_image:"/assets/black0.png".into(), white_stone_image:"/assets/white0.png".into(),
        ..theme.clone()
    };
    wb.set_appearance_settings(AppearanceSettings { theme:"Personal".into(), ..Default::default() },edited.clone()).unwrap();
    assert_eq!(appearance::load_theme(wb.storage(),"Personal",&ThemeSettings::default()),edited);
}
#[cfg(unix)]
#[test]
fn a_theme_folder_that_cannot_be_written_reports_the_settings_save_failure() {
    use std::os::unix::fs::PermissionsExt;
    let dir=tempfile::tempdir().unwrap(); let mut wb=workbench(dir.path());
    wb.add_theme("Personal").unwrap();
    let folder=appearance::theme_file(wb.storage(),"Personal").unwrap().parent().unwrap().to_path_buf();
    let before=wb.view(true).settings;
    let theme=appearance::load_theme(wb.storage(),"Personal",&ThemeSettings::default());
    let set=|mode| std::fs::set_permissions(&folder,std::fs::Permissions::from_mode(mode)).unwrap();
    set(0o555);
    let failed=wb.set_appearance_settings(AppearanceSettings { theme:"Personal".into(), ..Default::default() },ThemeSettings { shadow_size:42, ..theme });
    set(0o755);
    let error=failed.unwrap_err();
    let mut lines=error.message.lines();
    assert_eq!(lines.next(),Some(SETTINGS_SAVE_FAILED), "{}", error.message);
    assert!(lines.next().is_some_and(|cause|!cause.trim().is_empty()), "{}", error.message);
    assert_eq!(wb.view(false).settings,before);
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
    // The Java classic and soft bundles list, hide on delete, and their names stay taken.
    for (name,upper) in [("java-classic","Java-Classic"),("java-soft","Java-Soft")] {
        assert!(appearance::catalog(&s,&ThemeSettings::default()).iter().any(|t|t.name==name));
        // Its values are Java's defaults whatever the ui section holds; its images are the default ones.
        let ui=ThemeSettings { shadow_size:20, board_image:"ui-board.png".into(), ..Default::default() };
        assert_eq!(appearance::load_theme(&s,name,&ui),ThemeSettings::default());
        wb.delete_theme(name).unwrap();
        assert!(!appearance::catalog(&s,&ThemeSettings::default()).iter().any(|t|t.name==name));
        assert!(wb.add_theme(upper).is_err());
    }
}
#[test]
fn custom_images_are_owned_copies_and_clear_restores_the_default() {
    let dir=tempfile::tempdir().unwrap(); let mut wb=workbench(dir.path());
    let external=dir.path().join("selected.png");
    std::fs::write(&external,include_bytes!("../../../ui/public/assets/board.png")).unwrap();
    let settings=AppearanceSettings { custom_board_image:external.to_string_lossy().into_owned(), ..Default::default() };
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
    let settings=AppearanceSettings { custom_board_image:external.to_string_lossy().into_owned(), ..Default::default() };
    assert!(wb.set_appearance_settings(settings,ThemeSettings::default()).is_err());
    // Neither the copy nor the folders made for it remain.
    assert!(!appearance::themes_dir(wb.storage()).exists());
    assert!(external.is_file());
}

/// 界面颜色 belong to the theme that sets them: the 默认 theme keeps its own in the settings, a
/// named theme keeps its own in its file and does not take the 默认 theme's, and an unset colour
/// leaves no key behind.
#[test]
fn interface_colours_belong_to_their_theme_and_an_unset_one_leaves_the_file() {
    let dir=tempfile::tempdir().unwrap(); let mut wb=workbench(dir.path());
    wb.add_theme("Personal").unwrap();
    let defaults=ThemeSettings { ui_chrome_color:vec![30,30,30], ui_text_color:vec![230,230,230], glass_corner_radius:20, ..Default::default() };
    wb.set_appearance_settings(AppearanceSettings::default(),defaults.clone()).unwrap();
    assert_eq!(workbench(dir.path()).view(true).settings.theme,defaults);
    // The template has no glass-corner-radius, so that one still comes from the 默认 theme.
    let personal=appearance::load_theme(wb.storage(),"Personal",&wb.config().appearance_defaults);
    assert_eq!((personal.ui_chrome_color.len(),personal.ui_text_color.len(),personal.glass_corner_radius),(0,0,20));
    let file=appearance::theme_file(wb.storage(),"Personal").unwrap();
    let read=|| serde_json::from_slice::<serde_json::Value>(&std::fs::read(&file).unwrap()).unwrap();
    let settings=AppearanceSettings { theme:"Personal".into(), ..Default::default() };
    wb.set_appearance_settings(settings.clone(),ThemeSettings { ui_accent_color:vec![200,60,40], ..personal.clone() }).unwrap();
    assert_eq!(read()["ui-accent-color"],serde_json::json!([200,60,40]));
    assert_eq!(wb.view(false).settings.theme.ui_accent_color,vec![200,60,40]);
    wb.set_appearance_settings(settings,personal.clone()).unwrap();
    assert!(read().get("ui-accent-color").is_none(), "{}", read());
    assert_eq!(wb.view(false).settings.theme,personal);
    // A colour no theme can use is reported and left at the theme's value.
    let raw=serde_json::json!({"ui-chrome-color":[1,2],"ui-text-color":[1,2,3,4],"ui-accent-color":[5,6,7]});
    let raw=raw.as_object().unwrap();
    assert_eq!(appearance::invalid_java_keys(raw),vec!["ui-chrome-color","ui-text-color"]);
    let merged=appearance::merge_java(&ThemeSettings::default(),raw);
    assert_eq!((merged.ui_chrome_color.len(),merged.ui_text_color.len(),merged.ui_accent_color),(0,0,vec![5,6,7]));
}

/// The 棋盘 and 棋子 choices: any theme, the 默认 one included, can name the program's own and
/// the bundled themes' pictures by address, and a save keeps the address.
#[test]
fn any_theme_shows_a_shipped_board_or_stones_named_by_address() {
    let dir=tempfile::tempdir().unwrap(); let mut wb=workbench(dir.path());
    let theme=ThemeSettings { board_image:"/themes/sabaki/board.png".into(), black_stone_image:"/themes/Fast/black0.png".into(),
        white_stone_image:"/assets/white0.png".into(), ..Default::default() };
    wb.set_appearance_settings(AppearanceSettings::default(),theme.clone()).unwrap();
    let s=workbench(dir.path()).view(true).settings;
    assert_eq!(s.theme,theme);
    let shown=appearance::images(wb.storage(),&s.appearance,&s.theme);
    assert_eq!((shown.board.as_str(),shown.black.as_str(),shown.white.as_str()),("/themes/sabaki/board.png","/themes/Fast/black0.png","/assets/white0.png"));
    wb.add_theme("Personal").unwrap();
    let personal=AppearanceSettings { theme:"Personal".into(), ..Default::default() };
    let named=ThemeSettings { board_image:"/themes/yasnaya/board7.jpg".into(), ..appearance::load_theme(wb.storage(),"Personal",&ThemeSettings::default()) };
    assert_eq!(appearance::images(wb.storage(),&personal,&named).board,"/themes/yasnaya/board7.jpg");
    // An address the program does not ship is no picture.
    let missing=ThemeSettings { board_image:"/themes/sabaki/missing.png".into(), ..Default::default() };
    assert_eq!(appearance::images(wb.storage(),&AppearanceSettings::default(),&missing).board,"/assets/board.png");
}

/// The 默认 theme's relative paths are read from the themes folder, as Java reads the ui section's
/// (Theme.pathPrefix).
#[test]
fn the_default_theme_reads_relative_image_paths_from_the_themes_folder() {
    let dir=tempfile::tempdir().unwrap(); let wb=workbench(dir.path());
    let folder=appearance::themes_dir(wb.storage());
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("mine.png"),include_bytes!("../../../ui/public/assets/newtheme/black.png")).unwrap();
    let theme=ThemeSettings { black_stone_image:"mine.png".into(), ..Default::default() };
    assert!(appearance::images(wb.storage(),&AppearanceSettings::default(),&theme).black.starts_with("data:image/png;base64,"));
}

/// The 棋盘 and 棋子 choices list every picture the program ships once, each address loadable.
#[test]
fn shipped_boards_and_stones_list_each_picture_once() {
    use sha2::{Digest, Sha256};
    let public=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/public");
    let picture=|address:&str| { assert!(appearance::shipped_image(address), "{address}"); Sha256::digest(std::fs::read(public.join(&address[1..])).unwrap()) };
    let boards=appearance::shipped_boards();
    let stones=appearance::shipped_stones();
    assert_eq!(boards[0].theme,"default"); assert_eq!(stones[0].theme,"default");
    let mut seen=Vec::new();
    for board in &boards {
        let first=picture(&board.addresses[0]);
        assert!(board.addresses.iter().all(|a| picture(a)==first)); assert!(!seen.contains(&first)); seen.push(first);
    }
    let mut seen=Vec::new();
    for set in &stones {
        let first=(picture(&set.pairs[0].0),picture(&set.pairs[0].1));
        assert!(set.pairs.iter().all(|(b,w)| (picture(b),picture(w))==first)); assert!(!seen.contains(&first)); seen.push(first);
    }
    // Every bundled theme's own board and stones are among them.
    let boards:Vec<String>=boards.into_iter().flat_map(|b|b.addresses).collect();
    let stones:Vec<String>=stones.into_iter().flat_map(|s|s.pairs.into_iter().flat_map(|(b,w)|[b,w])).collect();
    for theme in ["sabaki","yasnaya","Megapack","Custom","Fast"] {
        let raw:serde_json::Value=serde_json::from_slice(&std::fs::read(public.join("themes").join(theme).join("theme.txt")).unwrap()).unwrap();
        let address=|key:&str| format!("/themes/{theme}/{}",raw[key].as_str().unwrap());
        assert!(boards.contains(&address("board-image")), "{theme}");
        assert!(stones.contains(&address("black-stone-image")) && stones.contains(&address("white-stone-image")), "{theme}");
    }
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

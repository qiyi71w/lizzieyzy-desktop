//! The panel 排列 in 自定义布局 slots, 恢复默认面板尺寸, 重置界面位置 and the Java import, through the
//! workbench operations, the settings projection, `config.json` and `persist.json`.
use std::path::Path;

use lizzie_core::api::ImportTarget;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::java_import::{self, JavaSource};
use lizzie_core::settings::{ArrangeablePanel as P, ArrangedPanel, PanelArrangement, PanelSizes, Storage};
use lizzie_core::workbench::{Workbench, IMPORT_NEEDS_REPLACE};
use serde_json::{json, Value};

struct NoEngines;
impl Launcher for NoEngines {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        Err(std::io::Error::other("no engines in arrangement tests"))
    }
}
fn storage(dir: &Path) -> Storage {
    Storage::in_dirs(&dir.join("config"), &dir.join("data"))
}
fn workbench(dir: &Path) -> Workbench {
    Workbench::new(storage(dir), Some("zh-CN"), Box::new(NoEngines), Vec::new())
}
fn column(panels: &[(P, f64)]) -> Vec<ArrangedPanel> {
    panels.iter().map(|&(panel, weight)| ArrangedPanel { panel, weight }).collect()
}
/// 胜率 on top of the left column, 选点列表 moved into it.
fn arrangement() -> PanelArrangement {
    PanelArrangement {
        left: column(&[(P::Winrate, 2.0), (P::Captured, 1.0), (P::CandidateList, 1.5), (P::SubBoard, 3.0)]),
        right: column(&[(P::VariationGraph, 1.0), (P::Comment, 0.75)]),
    }
}
/// Everything on the right, in another order.
fn other() -> PanelArrangement {
    PanelArrangement {
        left: Vec::new(),
        right: column(&[(P::Comment, 1.0), (P::Captured, 0.5), (P::Winrate, 2.0), (P::VariationGraph, 1.0), (P::CandidateList, 1.0), (P::SubBoard, 2.0)]),
    }
}
fn shown(wb: &mut Workbench) -> Option<PanelArrangement> {
    wb.view(false).settings.panel_arrangement
}
fn json_file(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
/// `persist.json`'s 排列.
fn persisted(dir: &Path) -> Option<PanelArrangement> {
    json_file(&storage(dir).persist_file).get("panelArrangement").map(|a| serde_json::from_value(a.clone()).unwrap())
}
/// `config.json`'s slot (1 or 2).
fn saved_slot(dir: &Path, slot: usize) -> Value {
    json_file(&storage(dir).config_file)["customLayouts"][slot - 1].clone()
}
/// SetCustomMode(slot) 确定 with the dialog left as it opened.
fn save_slot(wb: &mut Workbench, slot: u8) -> Result<bool, lizzie_core::api::ApiError> {
    let s = wb.view(false).settings;
    wb.commit_layout(s.extra_mode, s.panels, s.panel_sizes.board_position_proportion, Some((slot, None)))
}
/// Any write of `persist.json` fails while a directory takes its place.
fn block_persist(dir: &Path) -> Vec<u8> {
    let file = storage(dir).persist_file;
    let saved = std::fs::read(&file).unwrap();
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir_all(&file).unwrap();
    saved
}
fn unblock_persist(dir: &Path, saved: Vec<u8>) {
    let file = storage(dir).persist_file;
    std::fs::remove_dir(&file).unwrap();
    std::fs::write(&file, saved).unwrap();
}

#[test]
fn a_layout_slot_saves_the_arrangement_and_applying_it_restores_it() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let mut wb = workbench(dir);
    wb.set_panel_arrangement(Some(arrangement())).unwrap();
    assert!(save_slot(&mut wb, 1).unwrap());
    let slot = wb.view(false).settings.custom_layouts[0].clone().unwrap();
    assert_eq!(slot.panel_arrangement, Some(arrangement()));
    assert_eq!(serde_json::from_value::<PanelArrangement>(saved_slot(dir, 1)["panel_arrangement"].clone()).unwrap(), arrangement());

    wb.set_panel_arrangement(Some(other())).unwrap();
    assert!(wb.apply_custom_layout(1).unwrap());
    assert_eq!(shown(&mut wb), Some(arrangement()));
    assert_eq!(persisted(dir), Some(arrangement()));
    drop(wb);
    // The slot and the arrangement it applied survive a restart.
    let mut wb = workbench(dir);
    assert_eq!(shown(&mut wb), Some(arrangement()));
    wb.set_panel_arrangement(Some(other())).unwrap();
    wb.apply_custom_layout(1).unwrap();
    assert_eq!(shown(&mut wb), Some(arrangement()));
}

#[test]
fn saving_and_applying_a_slot_are_each_one_write_with_the_arrangement() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let mut wb = workbench(dir);
    wb.set_panel_arrangement(Some(arrangement())).unwrap();
    // Saving: a failed write leaves the slot empty.
    let saved = block_persist(dir);
    assert!(save_slot(&mut wb, 1).is_err());
    assert!(wb.view(false).settings.custom_layouts[0].is_none());
    unblock_persist(dir, saved);
    save_slot(&mut wb, 1).unwrap();
    // Applying: a failed write keeps the current arrangement and the settings file.
    wb.set_panel_arrangement(Some(other())).unwrap();
    let config = std::fs::read(storage(dir).config_file).unwrap();
    let saved = block_persist(dir);
    assert!(wb.apply_custom_layout(1).is_err());
    assert_eq!(shown(&mut wb), Some(other()));
    assert_eq!(std::fs::read(storage(dir).config_file).unwrap(), config);
    unblock_persist(dir, saved);
    assert_eq!(persisted(dir), Some(other()));
}

#[test]
fn a_slot_without_an_arrangement_applies_the_java_layout() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let mut wb = workbench(dir);
    // Saved while no arrangement was committed.
    save_slot(&mut wb, 2).unwrap();
    assert!(wb.view(false).settings.custom_layouts[1].as_ref().unwrap().panel_arrangement.is_none());
    wb.set_panel_arrangement(Some(arrangement())).unwrap();
    assert!(wb.apply_custom_layout(2).unwrap());
    assert_eq!(shown(&mut wb), None);
    assert_eq!(persisted(dir), None);

    // A slot saved before slots kept the arrangement (no field), and one this version cannot
    // use: config.json still loads, and applying either shows the Java layout.
    wb.set_panel_arrangement(Some(arrangement())).unwrap();
    save_slot(&mut wb, 1).unwrap();
    drop(wb);
    let file = storage(dir).config_file;
    let duplicate = json!({ "left": [{ "panel": "Captured", "weight": 1.0 }, { "panel": "Captured", "weight": 1.0 }], "right": [] });
    for stored in [None, Some(duplicate)] {
        let mut config = json_file(&file);
        let slot = config["customLayouts"][0].as_object_mut().unwrap();
        match &stored {
            None => slot.remove("panel_arrangement"),
            Some(value) => slot.insert("panel_arrangement".into(), value.clone()),
        };
        std::fs::write(&file, config.to_string()).unwrap();
        let mut wb = workbench(dir);
        let s = wb.view(false).settings;
        assert_eq!(s.custom_layouts[0].as_ref().unwrap().panel_arrangement, None, "{stored:?}");
        assert_eq!(s.panel_arrangement, Some(arrangement()));
        assert!(wb.apply_custom_layout(1).unwrap());
        assert_eq!(shown(&mut wb), None, "{stored:?}");
        assert_eq!(persisted(dir), None);
        wb.set_panel_arrangement(Some(arrangement())).unwrap();
    }
}

#[test]
fn restoring_default_sizes_gives_default_weights_in_the_same_columns_and_order() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let mut wb = workbench(dir);
    let sizes = PanelSizes { leftover_left_share: Some(0.3), board_position_proportion: 2, comment_height_share: Some(0.6), variation_graph_share: Some(0.8) };
    wb.set_panel_sizes(sizes).unwrap();
    wb.set_panel_arrangement(Some(arrangement())).unwrap();
    // The Java layout's heights at 1500×900 with the default sizes.
    let restored = PanelArrangement {
        left: column(&[(P::Winrate, 323.0), (P::Captured, 93.0), (P::CandidateList, 191.0), (P::SubBoard, 365.0)]),
        right: column(&[(P::VariationGraph, 191.0), (P::Comment, 256.0)]),
    };
    // Both in one write: when it fails, neither changes.
    let saved = block_persist(dir);
    assert!(wb.restore_default_panel_sizes().is_err());
    let s = wb.view(false).settings;
    assert_eq!((s.panel_sizes, s.panel_arrangement), (sizes, Some(arrangement())));
    unblock_persist(dir, saved);

    assert!(wb.restore_default_panel_sizes().unwrap());
    let s = wb.view(false).settings;
    assert_eq!((s.panel_sizes, s.panel_arrangement), (PanelSizes::default(), Some(restored.clone())));
    let disk = json_file(&storage(dir).persist_file);
    assert_eq!(serde_json::from_value::<PanelSizes>(disk["panelSizes"].clone()).unwrap(), PanelSizes::default());
    assert_eq!(persisted(dir), Some(restored.clone()));
    drop(wb);
    let mut wb = workbench(dir);
    assert_eq!(shown(&mut wb), Some(restored.clone()));
    assert!(!wb.restore_default_panel_sizes().unwrap());

    // Default sizes with dragged weights: only the weights go back. Without an arrangement
    // there is still none.
    let mut dragged = restored.clone();
    dragged.right[0].weight = 400.0;
    wb.set_panel_arrangement(Some(dragged)).unwrap();
    assert!(wb.restore_default_panel_sizes().unwrap());
    assert_eq!(shown(&mut wb), Some(restored));
    wb.set_panel_arrangement(None).unwrap();
    wb.set_panel_sizes(sizes).unwrap();
    wb.restore_default_panel_sizes().unwrap();
    assert_eq!(shown(&mut wb), None);
    assert_eq!(persisted(dir), None);
}

#[test]
fn resetting_the_interface_position_clears_the_arrangement() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let mut wb = workbench(dir);
    wb.set_panel_arrangement(Some(arrangement())).unwrap();
    wb.reset_ui_position().unwrap();
    assert_eq!(shown(&mut wb), None);
    assert!(!storage(dir).persist_file.exists());
    // Until the next start nothing is written to the persist file.
    wb.set_panel_arrangement(Some(other())).unwrap();
    assert!(!storage(dir).persist_file.exists());
    drop(wb);
    assert_eq!(shown(&mut workbench(dir)), None);
}

/// A Java work directory with `ui` settings in config.txt and `ui-persist` values in persist.
fn java_dir(root: &Path, ui: Value, ui_persist: Value) -> std::path::PathBuf {
    let java = root.join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), json!({ "ui": ui }).to_string()).unwrap();
    std::fs::write(java.join("persist"), json!({ "ui-persist": ui_persist }).to_string()).unwrap();
    java
}
fn preview(wb: &mut Workbench, java: &Path) -> lizzie_core::api::ImportPreviewView {
    wb.preview_java_import(java_import::read(&JavaSource::new(java, None)).unwrap())
}

#[test]
fn a_java_import_with_panel_sizes_clears_the_arrangement() {
    let tmp = tempfile::tempdir().unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product);
    wb.set_panel_arrangement(Some(arrangement())).unwrap();
    // The Java sizes equal this product's: the arrangement they replace still needs confirmation.
    let java = java_dir(tmp.path(), json!({ "play-sound": false }), json!({ "board-postion-propotion": 4 }));
    let p = preview(&mut wb, &java);
    assert!(p.replaces.contains(&ImportTarget::PanelSizes));
    assert_eq!(wb.apply_java_import(p.id, &[], false, false, false, None).unwrap(), Err(IMPORT_NEEDS_REPLACE));
    assert_eq!(shown(&mut wb), Some(arrangement()));
    assert_eq!(persisted(&product), Some(arrangement()));

    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    assert_eq!(shown(&mut wb), None);
    assert_eq!(persisted(&product), None);
    drop(wb);
    assert_eq!(shown(&mut workbench(&product)), None);
}

#[test]
fn a_java_import_without_panel_sizes_keeps_the_arrangement_and_imported_slots_have_none() {
    let tmp = tempfile::tempdir().unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product);
    wb.set_panel_arrangement(Some(arrangement())).unwrap();
    save_slot(&mut wb, 1).unwrap();
    let snapshot = json!({ "show-subboard": true, "show-winrate-graph": true, "show-comment": true, "show-list-pane": true, "board-position-proportion": 4 });
    let java = java_dir(tmp.path(), json!({ "play-sound": false, "custom-layout-1": snapshot }), json!({ "window-maximized": false }));
    let p = preview(&mut wb, &java);
    assert!(!p.replaces.contains(&ImportTarget::PanelSizes));
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    assert_eq!(shown(&mut wb), Some(arrangement()));
    assert_eq!(persisted(&product), Some(arrangement()));
    // The imported slot replaced the saved one, without an arrangement.
    assert_eq!(wb.view(false).settings.custom_layouts[0].as_ref().unwrap().panel_arrangement, None);
    assert!(saved_slot(&product, 1)["panel_arrangement"].is_null());
    assert!(wb.apply_custom_layout(1).unwrap());
    assert_eq!(shown(&mut wb), None);
    assert_eq!(persisted(&product), None);
}

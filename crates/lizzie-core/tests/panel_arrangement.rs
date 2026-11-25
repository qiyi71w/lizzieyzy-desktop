//! The panel 排列 through the command operation, the settings projection and `persist.json`.
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::settings::{ArrangeablePanel as P, ArrangedPanel, PanelArrangement, PanelSizes, Storage};
use lizzie_core::workbench::Workbench;

struct NoEngines;
impl Launcher for NoEngines {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        Err(std::io::Error::other("no engines in arrangement tests"))
    }
}
fn storage(dir: &std::path::Path) -> Storage {
    Storage::in_dirs(&dir.join("config"), &dir.join("data"))
}
fn workbench(dir: &std::path::Path) -> Workbench {
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
fn shown(wb: &mut Workbench) -> Option<PanelArrangement> {
    wb.view(false).settings.panel_arrangement
}
fn on_disk(dir: &std::path::Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(storage(dir).persist_file).unwrap()).unwrap()
}

#[test]
fn committed_arrangement_matches_the_persist_file_and_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    assert_eq!(shown(&mut wb), None);
    assert!(wb.set_panel_arrangement(Some(arrangement())).unwrap());
    assert_eq!(shown(&mut wb), Some(arrangement()));
    let disk: PanelArrangement = serde_json::from_value(on_disk(dir.path())["panelArrangement"].clone()).unwrap();
    assert_eq!(disk, arrangement());
    drop(wb);
    let mut wb = workbench(dir.path());
    assert_eq!(shown(&mut wb), Some(arrangement()));
    // None goes back to the Java layout, on disk too and after a restart.
    assert!(wb.set_panel_arrangement(None).unwrap());
    assert_eq!(shown(&mut wb), None);
    assert!(on_disk(dir.path()).get("panelArrangement").is_none());
    drop(wb);
    assert_eq!(shown(&mut workbench(dir.path())), None);
}

#[test]
fn invalid_arrangements_are_refused_and_change_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.set_panel_arrangement(Some(arrangement())).unwrap();
    let before = std::fs::read(storage(dir.path()).persist_file).unwrap();
    let with = |change: &dyn Fn(&mut PanelArrangement)| {
        let mut a = arrangement();
        change(&mut a);
        a
    };
    let invalid = [
        with(&|a| a.right[1].panel = P::Captured),
        with(&|a| { a.right.pop(); }),
        with(&|a| a.right.push(ArrangedPanel { panel: P::Comment, weight: 1.0 })),
        with(&|a| a.left[0].weight = f64::NAN),
        with(&|a| a.left[0].weight = f64::INFINITY),
        with(&|a| a.left[1].weight = 0.0),
        with(&|a| a.right[0].weight = -1.0),
    ];
    for a in invalid {
        assert!(wb.set_panel_arrangement(Some(a.clone())).is_err(), "{a:?}");
        assert_eq!(shown(&mut wb), Some(arrangement()));
    }
    assert_eq!(std::fs::read(storage(dir.path()).persist_file).unwrap(), before);
    // A column may be empty.
    let all_right = PanelArrangement { left: Vec::new(), right: [arrangement().left, arrangement().right].concat() };
    assert!(wb.set_panel_arrangement(Some(all_right.clone())).unwrap());
    assert_eq!(shown(&mut wb), Some(all_right));
}

#[test]
fn an_unchanged_arrangement_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    assert!(!wb.set_panel_arrangement(None).unwrap());
    assert!(!storage(dir.path()).persist_file.exists());
    wb.set_panel_arrangement(Some(arrangement())).unwrap();
    // A directory in its place makes any write fail, so only a skipped write succeeds.
    let file = storage(dir.path()).persist_file;
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir_all(&file).unwrap();
    assert_eq!(wb.set_panel_arrangement(Some(arrangement())).unwrap(), false);
}

/// A divider drag's weights, as the UI's JSON gives them (shortest round-trip decimals), come
/// back from the published settings and `persist.json` as the same decimals: the UI keeps its
/// draft until the view carries exactly the arrangement it sent.
#[test]
fn a_committed_arrangement_comes_back_with_the_exact_weights_the_ui_sent() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let sent = r#"{"type":"SetPanelArrangement","arrangement":{"left":[{"panel":"Captured","weight":93},{"panel":"Winrate","weight":200.66666666666723},{"panel":"Comment","weight":100.33333333333307}],"right":[{"panel":"SubBoard","weight":365},{"panel":"VariationGraph","weight":191},{"panel":"CandidateList","weight":191}]}}"#;
    let lizzie_core::api::Command::SetPanelArrangement { arrangement } = serde_json::from_str(sent).unwrap() else { panic!("not SetPanelArrangement") };
    assert!(wb.set_panel_arrangement(arrangement).unwrap());
    let published = serde_json::to_string(&shown(&mut wb)).unwrap();
    let persisted = std::fs::read_to_string(storage(dir.path()).persist_file).unwrap();
    for weights in [published, persisted] {
        assert!(weights.contains("200.66666666666723") && weights.contains("100.33333333333307"), "{weights}");
    }
}

#[test]
fn a_failed_write_keeps_the_previous_arrangement() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    wb.set_panel_arrangement(Some(arrangement())).unwrap();
    let file = storage(dir.path()).persist_file;
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir_all(&file).unwrap();
    let mut other = arrangement();
    other.left.swap(0, 1);
    assert!(wb.set_panel_arrangement(Some(other)).is_err());
    assert_eq!(shown(&mut wb), Some(arrangement()));
    assert!(wb.set_panel_arrangement(None).is_err());
    assert_eq!(shown(&mut wb), Some(arrangement()));
}

#[test]
fn a_persist_file_without_the_field_or_with_an_unusable_one_still_loads() {
    let dir = tempfile::tempdir().unwrap();
    let file = storage(dir.path()).persist_file;
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    let sizes = serde_json::json!({ "leftoverLeftShare": 0.3, "boardPositionProportion": 2, "commentHeightShare": 0.6, "variationGraphShare": null });
    std::fs::write(&file, serde_json::json!({ "lastFolder": "/records", "panelSizes": sizes, "bottomToolbar": "Hidden" }).to_string()).unwrap();
    let mut wb = workbench(dir.path());
    let s = wb.view(false).settings;
    assert_eq!(s.panel_arrangement, None);
    assert_eq!(s.panel_sizes, PanelSizes { leftover_left_share: Some(0.3), board_position_proportion: 2, comment_height_share: Some(0.6), variation_graph_share: None });
    drop(wb);
    // An arrangement this version cannot use reads as none; the rest of the file is kept.
    let duplicate = serde_json::json!({ "left": [{ "panel": "Captured", "weight": 1.0 }, { "panel": "Captured", "weight": 1.0 }], "right": [] });
    let unknown = serde_json::json!({ "left": [{ "panel": "Clock", "weight": 1.0 }], "right": [] });
    for stored in [duplicate, unknown] {
        std::fs::write(&file, serde_json::json!({ "panelSizes": sizes, "bottomToolbar": "Hidden", "panelArrangement": stored }).to_string()).unwrap();
        let s = workbench(dir.path()).view(false).settings;
        assert_eq!(s.panel_arrangement, None);
        assert_eq!(s.panel_sizes.board_position_proportion, 2);
    }
}

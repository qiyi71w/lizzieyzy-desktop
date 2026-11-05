use super::*;
use crate::{record::Stone, settings::DisplaySettings};
use serde_json::{json, Value};
fn fixtures() -> (Value, Value) {
    (
        serde_json::from_str(include_str!("../../tests/fixtures/strength/spec.json")).unwrap(),
        serde_json::from_str(include_str!("../../tests/fixtures/strength/java-golden.json")).unwrap(),
    )
}
fn close(actual: f64, expected: f64, path: &str) {
    assert!(
        (actual - expected).abs() <= 1e-9 * actual.abs().max(expected.abs()).max(1.0),
        "{path}: {actual} != {expected}"
    );
}
fn compare(actual: &Value, expected: &Value, path: &str) {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => {
            for (k, v) in e {
                if k == "rank_value_text" || k == "quality_score_text" {
                    continue;
                }
                compare(
                    a.get(k).unwrap_or_else(|| panic!("missing {path}.{k}")),
                    v,
                    &format!("{path}.{k}"),
                );
            }
        }
        (Value::Array(e), Value::Array(a)) => {
            assert_eq!(a.len(), e.len(), "{path}");
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                compare(a, e, &format!("{path}[{i}]"));
            }
        }
        (Value::Number(e), Value::Number(a)) => close(a.as_f64().unwrap(), e.as_f64().unwrap(), path),
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
fn input(scenario: &Value) -> MainLineInput {
    let kata = scenario["kata"].as_bool().unwrap();
    let positions = scenario["positions"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let color = if p["color"] == "W" { Stone::White } else { Stone::Black };
            let action = (i > 0).then(|| MoveInput {
                color,
                move_number: i as u32,
                coordinate: p["move"].as_str().unwrap().into(),
                ordinary: true,
            });
            let analysis = p["analysis"].as_object().map(|_| {
                let a = &p["analysis"];
                let kata = kata && !a["candidates"].as_array().unwrap().is_empty();
                AnalysisInput {
                    to_play: if i == 0 { Stone::Black } else { color.opposite() },
                    winrate: a["winrate"].as_f64().unwrap(),
                    score_lead: Some(a["scoreMean"].as_f64().unwrap()),
                    kata,
                    has_payload: true,
                    candidates: a["candidates"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|c| CandidateInput {
                            coordinate: c["move"].as_str().unwrap().into(),
                            order: c["order"].as_u64().unwrap() as u32,
                            visits: c["playouts"].as_u64().unwrap(),
                            winrate: c["winrate"].as_f64().unwrap(),
                            score_lead: Some(c["scoreMean"].as_f64().unwrap()),
                            kata,
                            prior: c["policy"].as_f64().unwrap() / 100.0,
                        })
                        .collect(),
                    extra_candidates: vec![],
                }
            });
            PositionInput { action, analysis }
        })
        .collect();
    MainLineInput {
        board_width: 19,
        board_height: 19,
        game: json!({"rules":"chinese","komi":7.5}),
        has_setup_stones: false,
        positions,
    }
}
fn projected(side: &StrengthSideReport) -> Value {
    let mut value = serde_json::to_value(&side.summary).unwrap();
    value["confidence"] = json!(format!("{:?}", side.summary.confidence).to_uppercase());
    value["samples"] = json!(side
        .samples
        .iter()
        .map(|s| {
            let mut v = serde_json::to_value(s).unwrap();
            v["color"] = json!(if s.color == Stone::Black { "B" } else { "W" });
            v["category"] = json!(format!("{:?}", s.category).to_uppercase());
            v["move_rank"] = json!(format!("{:?}", s.move_rank).to_uppercase());
            v
        })
        .collect::<Vec<_>>());
    value["segments"] = json!(side
        .segments
        .iter()
        .map(|s| {
            let mut summary = serde_json::to_value(&s.summary).unwrap();
            summary["confidence"] = json!(format!("{:?}", s.summary.confidence).to_uppercase());
            json!({"first_move":s.first_move,"last_move":s.last_move,"summary":summary})
        })
        .collect::<Vec<_>>());
    value["minimum_segment_samples"] = json!(side.minimum_segment_samples);
    value
}
#[test]
fn fixed_java_ordinary_reports() {
    let (spec, golden) = fixtures();
    let settings = DisplaySettings::default();
    assert_eq!(settings.win_loss_thresholds, [-1.0, -3.0, -6.0, -12.0, -24.0]);
    assert_eq!(settings.score_loss_thresholds, [-0.5, -1.5, -3.0, -6.0, -12.0]);
    for (scenario, expected) in spec["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .zip(golden["scenarios"].as_array().unwrap())
    {
        let result = estimate_with_model(&input(scenario), &StrengthModel::xgboost20tun(), &settings);
        assert_eq!(result.enough_data, expected["enough_data"].as_bool().unwrap());
        for (name, side) in [
            ("black", &result.black),
            ("white", &result.white),
            ("overall", &result.overall),
        ] {
            let mut e = expected[name].clone();
            e["samples"]
                .as_array_mut()
                .unwrap()
                .sort_by_key(|s| s["move_number"].as_u64().unwrap());
            compare(&projected(side), &e, &format!("{}.{}", scenario["name"], name));
        }
    }
}
#[test]
fn fixed_java_booster_and_calibrator() {
    let (spec, golden) = fixtures();
    let model = StrengthModel::xgboost20tun();
    for (i, v) in spec["xgboost20tun_full29"].as_array().unwrap().iter().enumerate() {
        let v: Vec<_> = v.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let full: [f64; 29] = v.try_into().unwrap();
        let e = &golden["xgboost20tun"][i];
        close(
            model.predict(&full).unwrap(),
            e["rank_value"].as_f64().unwrap(),
            "booster",
        );
        close(
            calibrator::bundled().apply(5.0, &full),
            e["calibrate_5"].as_f64().unwrap(),
            "calibrate5",
        );
        close(
            calibrator::bundled().apply(9.5, &full),
            e["calibrate_9_5"].as_f64().unwrap(),
            "calibrate9.5",
        );
    }
    for (i, v) in spec["humansl40_features"].as_array().unwrap().iter().enumerate() {
        let v: Vec<_> = v.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        close(
            humansl40::predict_vector(&v).unwrap(),
            golden["humansl40_predict"][i].as_f64().unwrap(),
            "human booster",
        );
    }
}
#[test]
fn fixed_java_human_features_and_rank() {
    let (spec, golden) = fixtures();
    for (i, g) in spec["humansl40_games"].as_array().unwrap().iter().enumerate() {
        let all = humansl40::extract(&g["moves"], &g["base"], &g["policies"], 19, false).unwrap();
        for (j, color) in ["B", "W"].iter().enumerate() {
            let moves: Vec<_> = all.iter().filter(|m| m.color == *color).cloned().collect();
            let expected = &golden["humansl40_games"][i][j];
            assert_eq!(moves.len(), expected["samples"].as_u64().unwrap() as usize);
            close(
                humansl40::predict(&moves).unwrap(),
                expected["rank"].as_f64().unwrap(),
                "human rank",
            );
            let f = human_features(&moves);
            for (k, actual) in f.iter().enumerate() {
                if expected["features"][k].is_null() {
                    assert!(actual.is_nan());
                } else {
                    close(
                        *actual,
                        expected["features"][k].as_f64().unwrap(),
                        &format!("human feature {k}"),
                    );
                }
            }
        }
    }
}
#[test]
fn missing_analysis_and_small_reports() {
    let (spec, _) = fixtures();
    let mut input = input(&spec["scenarios"][0]);
    input.positions.truncate(6);
    let mut result = estimate_with_model(&input, &StrengthModel::xgboost20tun(), &DisplaySettings::default());
    assert!(!result.enough_data);
    input.positions.iter_mut().for_each(|p| p.analysis = None);
    result = estimate_with_model(&input, &StrengthModel::xgboost20tun(), &DisplaySettings::default());
    assert_eq!(result.overall.summary.sample_count, 0);
    input.board_width = 13;
    assert_eq!(
        capture(&input, &DisplaySettings::default()).unwrap_err(),
        "HumanSL40.boardSize"
    );
}

#[test]
fn capture_limits_analysis_validation_and_snapshot() {
    let (spec, _) = fixtures();
    let settings = DisplaySettings::default();
    let mut input = input(&spec["scenarios"][4]);
    let captured = capture(&input, &settings).unwrap();
    assert_eq!(captured.game["moves"].as_array().unwrap().len(), 210);
    assert_eq!(captured.base.as_array().unwrap().len(), 211);
    assert!(captured.ordinary.is_some());
    let prepared = preparation_snapshot(&input, &settings).unwrap();
    assert_eq!(prepared.game["moves"].as_array().unwrap().len(), 230);
    assert!(prepared.ordinary.is_none());
    assert!(prepared.base.as_array().unwrap().iter().all(Value::is_null));
    input.has_setup_stones = true;
    assert_eq!(capture(&input, &settings).unwrap_err(), "HumanSL40.setupUnsupported");
    input.has_setup_stones = false;
    input.positions[1].action.as_mut().unwrap().ordinary = false;
    assert_eq!(capture(&input, &settings).unwrap_err(), "HumanSL40.setupUnsupported");
    input.positions[1].action.as_mut().unwrap().ordinary = true;
    input.positions.truncate(6);
    assert_eq!(capture(&input, &settings).unwrap_err(), "HumanSL40.needAnalysis");
    let mut analysis = input.positions.iter().find_map(|p| p.analysis.clone()).unwrap();
    analysis.candidates[0].prior = 0.25;
    analysis.extra_candidates.push(analysis.candidates[0].clone());
    let snap = position_snapshot(Some(&analysis));
    assert_eq!(snap["moveInfos"].as_array().unwrap().len(), analysis.candidates.len());
    assert_eq!(snap["moveInfos"][0]["prior"], 0.25);
    analysis.kata = false;
    assert!(position_snapshot(Some(&analysis)).is_null());
}

#[test]
fn imported_bundle_feature_metadata_and_default_calibration() {
    use std::{fs, io::Write};
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("bundle.zip");
    let mut archive = zip::ZipWriter::new(fs::File::create(&source).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    archive.start_file("nested/custom-booster.json", options).unwrap();
    archive
        .write_all(include_bytes!("../../resources/strength/xgboost20tun_booster.json"))
        .unwrap();
    archive.start_file("nested/metadata.json", options).unwrap();
    let metadata = json!({"display_name":"Custom strength", "feature_order":DEFAULT_FEATURE_INDICES.iter().map(|&i|FULL_FEATURE_NAMES[i]).collect::<Vec<_>>(), "prediction_sigma":1.25});
    archive.write_all(metadata.to_string().as_bytes()).unwrap();
    archive.finish().unwrap();
    let models = temp.path().join("models");
    let imported = import_model(&source, &models).unwrap();
    assert_eq!(imported.name, "Custom strength");
    assert_eq!(imported.prediction_sigma, 1.25);
    close(
        imported.predict(&[0.8; 29]).unwrap(),
        StrengthModel::xgboost20tun().predict(&[0.8; 29]).unwrap(),
        "implicit bundled calibrator",
    );
    let bad = temp.path().join("bad.json");
    fs::write(
        &bad,
        include_bytes!("../../resources/strength/xgboost20tun_booster.json"),
    )
    .unwrap();
    fs::write(temp.path().join("metadata.json"), r#"{"feature_order":["unknown"]}"#).unwrap();
    assert_eq!(
        import_model(&bad, &models).unwrap_err().to_string(),
        "Unknown feature in metadata feature_order: unknown"
    );
    assert_eq!(selectable_models(&models, &[]).len(), 3);
}

#[test]
fn human_report_uses_policies_and_segment_features() {
    let (spec, golden) = fixtures();
    let game = &spec["humansl40_games"][0];
    let moves = game["moves"].as_array().unwrap();
    let positions = game["base"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let to_play = if i < moves.len() {
                if moves[i][0] == "B" {
                    Stone::Black
                } else {
                    Stone::White
                }
            } else if moves[i - 1][0] == "B" {
                Stone::White
            } else {
                Stone::Black
            };
            PositionInput {
                action: (i > 0).then(|| MoveInput {
                    color: if moves[i - 1][0] == "B" {
                        Stone::Black
                    } else {
                        Stone::White
                    },
                    move_number: i as u32,
                    coordinate: moves[i - 1][1].as_str().unwrap().into(),
                    ordinary: true,
                }),
                analysis: Some(AnalysisInput {
                    to_play,
                    winrate: p["rootInfo"]["winrate"].as_f64().unwrap() * 100.0,
                    score_lead: Some(p["rootInfo"]["scoreMean"].as_f64().unwrap()),
                    kata: true,
                    has_payload: true,
                    candidates: p["moveInfos"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|c| CandidateInput {
                            coordinate: c["move"].as_str().unwrap().into(),
                            order: c["order"].as_u64().unwrap_or(0) as u32,
                            visits: 1,
                            winrate: 50.0,
                            score_lead: Some(c["scoreMean"].as_f64().unwrap()),
                            kata: true,
                            prior: c["prior"].as_f64().unwrap_or(0.0),
                        })
                        .collect(),
                    extra_candidates: vec![],
                }),
            }
        })
        .collect();
    let input = MainLineInput {
        board_width: 19,
        board_height: 19,
        game: json!({"rules":"chinese","komi":7.5}),
        has_setup_stones: false,
        positions,
    };
    let captured = capture(&input, &DisplaySettings::default()).unwrap();
    let result = report(&captured, &game["policies"], &DisplaySettings::default()).unwrap();
    assert!(result.enough_data);
    for (i, side) in [&result.black, &result.white].iter().enumerate() {
        close(
            side.summary.rank_value.unwrap(),
            golden["humansl40_games"][0][i]["rank"].as_f64().unwrap(),
            "enriched human report",
        );
        assert_eq!(side.minimum_segment_samples, 6);
        assert!(side.samples.iter().all(|s| s.human.is_some()));
        assert_eq!(side.summary.confidence, StrengthConfidence::Low);
    }
}

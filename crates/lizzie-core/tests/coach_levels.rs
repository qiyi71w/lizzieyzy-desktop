//! AI 陪练's opponents, move choice, think time and older saved settings against the values the
//! fixed Java produces: `fixtures/coach/java-coach.json` was written by running its
//! `training/CoachLevel`, `HumanSlPlaySettings`, `HumanThinkTime`, `HumanSlTrainingPreferences`
//! and `analysis/HumanPolicySampler`, `HumanSlChosenMove` on the inputs it lists.
use lizzie_core::humansl::{choose, think_time, Candidate, CoachLevel, CoachLevelKind, Pace};
use lizzie_core::settings::Config;
use serde_json::{json, Value};
use std::time::Duration;

fn java_table() -> Value {
    serde_json::from_str(include_str!("fixtures/coach/java-coach.json")).unwrap()
}

fn level(name: &str) -> CoachLevel {
    serde_json::from_value(json!(name)).unwrap()
}

fn candidates(pairs: &Value) -> Vec<Candidate> {
    pairs
        .as_array()
        .unwrap()
        .iter()
        .map(|p| Candidate {
            vertex: p[0].as_str().unwrap().into(),
            probability: p[1].as_f64().unwrap(),
        })
        .collect()
}

fn pace(name: &str) -> Pace {
    serde_json::from_value(json!(name)).unwrap()
}

#[test]
fn thirty_one_levels_match_the_java_table_and_default_to_three_dan() {
    let table = java_table();
    let rows = table["levels"].as_array().unwrap();
    assert_eq!(CoachLevel::ALL.len(), 31);
    assert_eq!(rows.len(), 31);
    assert_eq!(CoachLevel::default(), level(table["default"].as_str().unwrap()));
    assert_eq!(CoachLevel::default(), CoachLevel::Dan3);
    for (level, row) in CoachLevel::ALL.into_iter().zip(rows) {
        let name = row["name"].as_str().unwrap();
        assert_eq!(level, self::level(name), "order");
        let kind = match row["kind"].as_str().unwrap() {
            "KYU" => CoachLevelKind::Kyu,
            "DAN" => CoachLevelKind::Dan,
            _ => CoachLevelKind::StrongDan,
        };
        assert_eq!(
            (level.kind(), u64::from(level.rank()), level.fox()),
            (kind, row["rank"].as_u64().unwrap(), row["fox"].as_str().unwrap()),
            "{name}"
        );
        let p = level.play();
        let j = &row["play"];
        assert_eq!(p.profile, j["profile"], "{name}");
        assert_eq!(p.searches, j["searches"], "{name}");
        assert_eq!(u64::from(p.max_visits), j["maxVisits"].as_u64().unwrap(), "{name}");
        for (ours, key) in [
            (p.pikl_lambda, "piklLambda"),
            (p.root_explore_weightless, "rootExploreWeightless"),
            (p.cpuct_permanent, "cpuctPermanent"),
            (p.temperature_only_below_prob, "temperatureOnlyBelowProb"),
            (p.static_score_utility, "staticScoreUtility"),
            (p.dynamic_score_utility, "dynamicScoreUtility"),
            (p.temperature_early(), "temperatureEarly"),
            (p.temperature(), "temperature"),
            (p.temperature_halflife(), "temperatureHalflife"),
        ] {
            assert_eq!(ours, j[key].as_f64().unwrap(), "{name} {key}");
        }
        assert_eq!(p.ignore_pass(), j["ignorePass"], "{name}");
        assert_eq!(
            u64::from(p.move_time_factor),
            j["moveTimeFactor"].as_u64().unwrap(),
            "{name}"
        );
        // The search budget per move: 20 seconds for 强10段 and 11段, 10 for every other level.
        assert_eq!(
            p.move_budget(),
            Duration::from_secs(row["moveTimeoutSeconds"].as_u64().unwrap()),
            "{name}"
        );
        let strongest = matches!(level, CoachLevel::StrongDan10 | CoachLevel::Dan11);
        assert_eq!(
            p.move_budget(),
            Duration::from_secs(if strongest { 20 } else { 10 }),
            "{name}"
        );
    }
}

#[test]
fn older_numbered_ranks_map_to_the_plain_level_of_that_number() {
    for row in java_table()["forRank"].as_array().unwrap() {
        let (rank, dan) = (row["rank"].as_i64().unwrap(), row["dan"].as_bool().unwrap());
        assert_eq!(
            CoachLevel::for_rank(rank, dan),
            level(row["level"].as_str().unwrap()),
            "{rank} {dan}"
        );
    }
}

#[test]
fn policy_levels_sample_the_human_policy_as_java_does() {
    for row in java_table()["policy"].as_array().unwrap() {
        let moves = candidates(&row["moves"]);
        let number = row["moveNumber"].as_u64().unwrap() as u32;
        let (w, h) = (
            row["width"].as_u64().unwrap() as u16,
            row["height"].as_u64().unwrap() as u16,
        );
        assert_eq!(choose::top_share(&moves), row["topShare"].as_f64(), "{row}");
        for pick in row["picks"].as_array().unwrap() {
            let random = pick[0].as_f64().unwrap();
            assert_eq!(
                choose::policy_move(&moves, number, w, h, random),
                pick[1].as_str(),
                "{row} at {random}"
            );
        }
    }
}

#[test]
fn search_levels_choose_as_genmove_would_from_the_search() {
    let table = java_table();
    let infos = table["searchedMoveInfos"].as_array().unwrap();
    for row in table["searched"].as_array().unwrap() {
        let move_infos = infos[row["moveInfos"].as_u64().unwrap() as usize].as_array().unwrap();
        let human = candidates(&row["humanMoves"]);
        let play = level(row["level"].as_str().unwrap()).play();
        let number = row["moveNumber"].as_u64().unwrap() as u32;
        let allow_pass = row["allowPass"].as_bool().unwrap();
        for pick in row["picks"].as_array().unwrap() {
            let random = pick[0].as_f64().unwrap();
            let chosen = choose::searched_move(move_infos, &human, play, number, (19, 19), allow_pass, random);
            assert_eq!(chosen.as_deref(), pick[1].as_str(), "{row} at {random}");
        }
    }
}

#[test]
fn think_time_matches_java_for_each_pace() {
    for row in java_table()["thinkTime"].as_array().unwrap() {
        let millis = think_time(
            row["top"].as_f64(),
            row["moveNumber"].as_u64().unwrap() as u32,
            row["boardArea"].as_u64().unwrap() as usize,
            row["random"].as_f64().unwrap(),
            pace(row["pace"].as_str().unwrap()),
        );
        assert_eq!(millis, Duration::from_millis(row["millis"].as_u64().unwrap()), "{row}");
    }
}

#[test]
fn think_time_stays_in_each_pace_range_and_orders_the_paces() {
    let ms = |d: Duration| d.as_millis() as u64;
    for top in [None, Some(0.0), Some(0.05), Some(0.3), Some(0.6), Some(0.95), Some(1.0)] {
        for number in [0, 20, 36, 100, 250] {
            for random in [0.0, 0.25, 0.5, 0.75, 0.999] {
                let at = |p| ms(think_time(top, number, 361, random, p));
                let (natural, fast, slow) = (at(Pace::Natural), at(Pace::Fast), at(Pace::Slow));
                assert!((400..=8_000).contains(&natural), "{natural}");
                assert!((250..=1_800).contains(&fast), "{fast}");
                assert!((800..=14_000).contains(&slow), "{slow}");
                assert!(fast <= natural && natural <= slow, "{fast} {natural} {slow}");
            }
        }
    }
    for pace in [Pace::Natural, Pace::Fast, Pace::Slow] {
        // An obvious move (nearly everyone at the level plays it) is quicker than a hard one.
        assert!(think_time(Some(0.95), 100, 361, 0.5, pace) < think_time(Some(0.1), 100, 361, 0.5, pace));
        // The opening (the first tenth of the board) is quicker than later on.
        assert!(think_time(Some(0.4), 20, 361, 0.5, pace) < think_time(Some(0.4), 100, 361, 0.5, pace));
    }
}

/// This product's older settings file: the preset, the numbered rank and 用时 under its own names.
fn product_settings(java: &Value) -> lizzie_core::settings::HumanSlTrainingSettings {
    let mut training = serde_json::Map::new();
    for (java_key, key) in [
        ("opponent-preset", "opponent"),
        ("dan-rank", "danRank"),
        ("rank", "rank"),
        ("move-time-seconds", "moveTimeSeconds"),
        ("level", "level"),
        ("pace", "pace"),
    ] {
        if let Some(value) = java.get(java_key) {
            training.insert(key.into(), value.clone());
        }
    }
    let config: Config = serde_json::from_value(json!({ "humanSl": { "training": training } })).unwrap();
    config.human_sl.training
}

#[test]
fn older_saved_settings_read_as_java_reads_them() {
    for row in java_table()["saved"].as_array().unwrap() {
        let saved = &row["saved"];
        // An unreadable number falls back to the default 3 段 whatever 段/级 says (Java keeps its
        // fallback's number, 3, and so reads 3 级 here).
        if saved["rank"] == "x" {
            continue;
        }
        let s = product_settings(saved);
        assert_eq!(
            (s.level, s.pace),
            (
                level(row["level"].as_str().unwrap()),
                pace(row["pace"].as_str().unwrap())
            ),
            "{saved}"
        );
    }
    let s = product_settings(&json!({"rank": "x", "dan-rank": false}));
    assert_eq!(s.level, CoachLevel::Dan3);
}

#[test]
fn the_two_retired_presets_become_nine_dan_and_old_move_times_become_paces() {
    for preset in ["ModernPro", "Online9d"] {
        let s = product_settings(&json!({"opponent-preset": preset, "rank": 2, "dan-rank": false}));
        assert_eq!(s.level, CoachLevel::Dan9, "{preset}");
    }
    for (seconds, expected) in [
        (10, Pace::Natural),
        (30, Pace::Natural),
        (60, Pace::Slow),
        (86400, Pace::Slow),
        (17, Pace::Natural),
    ] {
        assert_eq!(
            product_settings(&json!({"move-time-seconds": seconds})).pace,
            expected,
            "{seconds}"
        );
    }
    let s = product_settings(&json!({"level": "Unknown", "pace": "Glacial"}));
    assert_eq!((s.level, s.pace), (CoachLevel::Dan3, Pace::Natural));
    // The new names round-trip.
    let saved = lizzie_core::settings::HumanSlTrainingSettings {
        level: CoachLevel::StrongDan7,
        pace: Pace::Fast,
        ..Default::default()
    };
    let back: lizzie_core::settings::HumanSlTrainingSettings =
        serde_json::from_value(serde_json::to_value(&saved).unwrap()).unwrap();
    assert_eq!(back, saved);
}

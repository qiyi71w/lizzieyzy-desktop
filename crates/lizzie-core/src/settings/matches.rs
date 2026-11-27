use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Java DesktopTimeControl.SideMode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum MatchTimeMode {
    EngineOwned,
    #[default]
    Fixed,
    RawAdvanced,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct MatchIdentity {
    pub commands: String,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct MatchResignPolicy {
    pub min_move: u32,
    pub consecutive_moves: u32,
    pub winrate: f64,
}

impl Default for MatchResignPolicy {
    fn default() -> Self {
        Self {
            min_move: 0,
            consecutive_moves: 2,
            winrate: 10.0,
        }
    }
}

/// Upper bounds of the opening breadth (`analysisWideRootNoise`, 0..=2) and aggressiveness
/// (`playoutDoublingAdvantage`, -3..=3), Java `EngineGameOpeningParameters.MAX_WRN` / `MAX_PDA`.
pub const MAX_OPENING_WRN: f64 = 2.0;
pub const MAX_OPENING_PDA: f64 = 3.0;

/// The opening breadth and aggressiveness of the two participant positions, 引擎 1 and 引擎 2 (Java
/// `EngineGameOpeningParameters`). They belong to positions, not to colours: exchanging colours
/// does not exchange them. Zero breadth turns the parameter off and zero aggressiveness is neutral.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct MatchOpeningParameters {
    pub first_wrn: f64,
    pub second_wrn: f64,
    pub first_pda: f64,
    pub second_pda: f64,
}

impl MatchOpeningParameters {
    pub fn valid_wrn(value: f64) -> bool {
        value.is_finite() && (0.0..=MAX_OPENING_WRN).contains(&value)
    }

    pub fn valid_pda(value: f64) -> bool {
        value.is_finite() && (-MAX_OPENING_PDA..=MAX_OPENING_PDA).contains(&value)
    }

    pub fn valid(&self) -> bool {
        [self.first_wrn, self.second_wrn].into_iter().all(Self::valid_wrn)
            && [self.first_pda, self.second_pda].into_iter().all(Self::valid_pda)
    }
}

/// The saved 引擎对战 values as a settings file holds them. A missing, unreadable or out-of-range
/// opening value is 0 (Java `EngineGameOpeningParameters.stored`), an unreadable rules choice is
/// none, and neither makes the file unusable.
pub(super) fn stored<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<MatchSettings, D::Error> {
    let mut value = serde_json::Value::deserialize(deserializer)?;
    if let Some(object) = value.as_object_mut() {
        if object.get("rulesChoice").is_some_and(|c| !c.is_string()) {
            object.remove("rulesChoice");
        }
        let opening = object.get("opening").and_then(serde_json::Value::as_object);
        let read = |key: &str, valid: fn(f64) -> bool| {
            opening
                .and_then(|o| o.get(key))
                .and_then(serde_json::Value::as_f64)
                .filter(|v| valid(*v))
                .unwrap_or(0.0)
        };
        let clean = MatchOpeningParameters {
            first_wrn: read("firstWrn", MatchOpeningParameters::valid_wrn),
            second_wrn: read("secondWrn", MatchOpeningParameters::valid_wrn),
            first_pda: read("firstPda", MatchOpeningParameters::valid_pda),
            second_pda: read("secondPda", MatchOpeningParameters::valid_pda),
        };
        object.insert(
            "opening".into(),
            serde_json::to_value(clean).map_err(serde::de::Error::custom)?,
        );
    }
    MatchSettings::deserialize(value).map_err(serde::de::Error::custom)
}

/// Persisted engine-match dialog values. disable-wrn-in-game remains owned by GameSettings and
/// is not read by engine matches; their opening breadth is [`MatchOpeningParameters`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct MatchSettings {
    pub first: MatchIdentity,
    pub second: MatchIdentity,
    pub komi: f64,
    pub handicap: u32,
    pub rules: String,
    /// The 本局规则 entry picked last (a `MATCH_RULE_OPTIONS` id), which tells the Chinese entries
    /// with the same rules apart; `None` for 自定义.
    pub rules_choice: Option<String>,
    pub ponder: bool,
    pub sgf_start: bool,
    pub sgf_random: bool,
    pub first_time_mode: MatchTimeMode,
    pub second_time_mode: MatchTimeMode,
    pub advanced_time: bool,
    pub first_time_command: String,
    pub second_time_command: String,
    pub first_resign: MatchResignPolicy,
    pub second_resign: MatchResignPolicy,
    pub check_random_visits: bool,
    pub random_visits_percent: f64,
    pub show_previous_bestmoves: bool,
    pub previous_bestmove_only: bool,
    /// 开局广度 / 开局激进度 of 引擎 1 and 引擎 2 (`engine-game-first-wrn` …).
    pub opening: MatchOpeningParameters,
}

impl Default for MatchSettings {
    fn default() -> Self {
        Self {
            first: MatchIdentity::default(),
            second: MatchIdentity::default(),
            komi: 7.5,
            handicap: 0,
            rules: String::new(),
            rules_choice: None,
            ponder: false,
            sgf_start: false,
            sgf_random: true,
            first_time_mode: MatchTimeMode::Fixed,
            second_time_mode: MatchTimeMode::Fixed,
            advanced_time: false,
            first_time_command: "time_settings 10 2 1".into(),
            second_time_command: "time_settings 10 2 1".into(),
            first_resign: MatchResignPolicy::default(),
            second_resign: MatchResignPolicy::default(),
            check_random_visits: false,
            random_visits_percent: 10.0,
            show_previous_bestmoves: false,
            previous_bestmove_only: true,
            opening: MatchOpeningParameters::default(),
        }
    }
}

pub(super) fn serialize_toolbar<S: serde::Serializer>(
    value: &crate::api::MatchSessionSettings,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut json = serde_json::to_value(value).map_err(serde::ser::Error::custom)?;
    if let Some(object) = json.as_object_mut() {
        for key in ["batchName", "startNumberEnabled", "startNumber"] {
            object.remove(key);
        }
    }
    json.serialize(serializer)
}

pub(super) fn deserialize_toolbar<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<crate::api::MatchSessionSettings, D::Error> {
    let mut value = crate::api::MatchSessionSettings::deserialize(deserializer)?;
    value.batch_name.clear();
    value.start_number_enabled = false;
    value.start_number = 1;
    Ok(value)
}

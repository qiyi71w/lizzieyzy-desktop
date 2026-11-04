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

/// Persisted engine-match dialog values. disable-wrn-in-game remains owned by GameSettings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct MatchSettings {
    pub first: MatchIdentity,
    pub second: MatchIdentity,
    pub komi: f64,
    pub handicap: u32,
    pub rules: String,
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
}

impl Default for MatchSettings {
    fn default() -> Self {
        Self {
            first: MatchIdentity::default(),
            second: MatchIdentity::default(),
            komi: 7.5,
            handicap: 0,
            rules: String::new(),
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

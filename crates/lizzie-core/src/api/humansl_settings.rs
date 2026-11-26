//! Persisted HumanSL preferences, independently validated training-model availability and the
//! opponents to choose from.
use crate::humansl::{CoachLevel, CoachLevelKind};
use crate::settings::HumanSlTrainingSettings;
use serde::Serialize;
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct HumanSlSettingsView {
    pub model: HumanSlModelView,
    pub training: HumanSlTrainingSettings,
    pub advanced_visible: bool,
    /// Every opponent, weakest first.
    pub levels: Vec<CoachLevelView>,
    /// The komi an even game would start with now: the even-game komi of the rules KataGo plays
    /// (`RulePreset.evenGameKomi`).
    pub even_komi: f64,
    /// The komi of a handicap game.
    pub handicap_komi: f64,
}

/// An opponent as the interface names it: by its Fox / Tygem rank ("野狐 3D"), then its KataGo
/// level ("3段", "强3段", "10级").
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct CoachLevelView {
    pub level: CoachLevel,
    pub fox: &'static str,
    pub kind: CoachLevelKind,
    pub rank: u8,
}

impl From<CoachLevel> for CoachLevelView {
    fn from(level: CoachLevel) -> Self {
        Self {
            level,
            fox: level.fox(),
            kind: level.kind(),
            rank: level.rank(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct HumanSlModelView {
    pub status: HumanSlModelStatus,
    pub path: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum HumanSlModelStatus {
    Missing,
    Installed,
}

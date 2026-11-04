//! Persisted HumanSL preferences and independently validated training-model availability.
use crate::settings::HumanSlTrainingSettings;
use serde::Serialize;
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct HumanSlSettingsView {
    pub model: HumanSlModelView,
    pub training: HumanSlTrainingSettings,
    pub advanced_visible: bool,
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

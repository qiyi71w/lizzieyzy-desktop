//! Cached-analysis strength estimation, independent of application state.
//!
//! The caller supplies root followed by main-line positions. Each non-root position carries
//! the action leading to it. `AnalysisInput` uses **side-to-move** winrate (0–100) and score;
//! candidates stay in Java bestMoves order. `prior` is a probability (0–1), unlike both
//! CandidateView.prior and Java MoveData.policy, which are percentages: adapt with `/ 100.0`.
//! Out-of-range candidates belong in `extra_candidates` (HumanSL snapshot only). No estimator starts analysis.
mod calibrator;
mod estimator;
mod humansl40;
mod models;
mod policy;
mod xgboost;

use crate::{display::MoveRank, record::Stone};
pub use estimator::{estimate, estimate_with_model, summarize_samples};
pub use humansl40::{human_features, HumanMove, HUMAN_SHA, PROFILES};
pub use models::{copy_java_models, effective_model, import_model, java_models, selectable_models, StrengthModel};
pub use policy::{
    cache_key, canonical, capture, fingerprint, has_analysis, policy_command, position_snapshot, preparation_snapshot,
    report, request, validate_game, validate_history, validate_policy, Capture, POLICY_CONFIG,
};
use serde::Serialize;
use ts_rs::TS;
pub use xgboost::{full_feature_index, Booster, DEFAULT_FEATURE_INDICES, FULL_FEATURE_NAMES};

#[derive(Clone, Debug, PartialEq)]
pub struct MainLineInput {
    pub board_width: u32,
    pub board_height: u32,
    /// AnalysisRequestBuilder-compatible game request (rules, komi, board sizes, moves, etc.).
    /// Capture replaces moves from positions and removes analyzeTurns/initialStones.
    pub game: serde_json::Value,
    pub has_setup_stones: bool,
    pub positions: Vec<PositionInput>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PositionInput {
    pub action: Option<MoveInput>,
    pub analysis: Option<AnalysisInput>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MoveInput {
    pub color: Stone,
    pub move_number: u32,
    /// GTP coordinate, or "pass".
    pub coordinate: String,
    /// Java isHistoryActionNode: false for setup/edit nodes.
    pub ordinary: bool,
}
/// Map NodeAnalysisView's to_play, winrate and score_lead without applying displayed
/// viewpoint/komi adjustments. `kata` records score-payload provenance (not the model name).
/// `has_payload` is Java's primary-payload predicate: visits/header/engine name/candidates/
/// KataGo/ownership, not a positive-visit requirement. An absent cached payload is `None`.
#[derive(Clone, Debug, PartialEq)]
pub struct AnalysisInput {
    pub to_play: Stone,
    pub winrate: f64,
    pub score_lead: Option<f64>,
    pub kata: bool,
    pub has_payload: bool,
    pub candidates: Vec<CandidateInput>,
    pub extra_candidates: Vec<CandidateInput>,
}
/// Map CandidateView's point to its GTP coordinate (None → pass), and order/visits/winrate/
/// score_lead directly. `kata` retains the score-payload provenance; `prior` is
/// CandidateView.prior / 100.0. Ignore `shown`, `side_to_move`, PV and ownership fields.
#[derive(Clone, Debug, PartialEq)]
pub struct CandidateInput {
    pub coordinate: String,
    pub order: u32,
    pub visits: u64,
    pub winrate: f64,
    pub score_lead: Option<f64>,
    pub kata: bool,
    /// Probability (0–1); multiply by 100 only when adapting to Java MoveData.policy.
    pub prior: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct StrengthModelView {
    pub id: String,
    pub name: String,
    pub human_sl: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct StrengthReport {
    pub model: StrengthModelView,
    pub black: StrengthSideReport,
    pub white: StrengthSideReport,
    pub overall: StrengthSideReport,
    pub enough_data: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct StrengthSideReport {
    pub summary: StrengthSummary,
    pub samples: Vec<StrengthSample>,
    pub segments: Vec<StrengthSegment>,
    pub minimum_segment_samples: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct StrengthSegment {
    pub first_move: u32,
    pub last_move: u32,
    pub summary: StrengthSummary,
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct StrengthSummary {
    pub sample_count: u32,
    pub score_sample_count: u32,
    pub rank_value: Option<f64>,
    pub prediction_sigma: Option<f64>,
    pub quality_score: Option<f64>,
    pub average_winrate_loss: f64,
    pub average_score_loss: f64,
    pub median_score_loss: f64,
    pub weighted_score_loss: f64,
    pub average_score_equivalent_loss: f64,
    pub match_rate: f64,
    pub first_choice_rate: f64,
    pub good_move_rate: f64,
    pub move_rank_good_move_rate: f64,
    pub move_rank_counts: Vec<u32>,
    pub bad_move_rate: f64,
    pub mistake_rate: f64,
    pub blunder_rate: f64,
    pub average_difficulty: Option<f64>,
    pub strength_band: String,
    pub confidence: StrengthConfidence,
    pub human_sl_coverage: Option<f64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum StrengthConfidence {
    Low,
    Medium,
    High,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum StrengthMoveCategory {
    Excellent,
    Great,
    Good,
    Inaccuracy,
    Mistake,
    Blunder,
}
impl StrengthMoveCategory {
    fn good(self) -> bool {
        matches!(self, Self::Excellent | Self::Great | Self::Good)
    }
    fn mistake(self) -> bool {
        matches!(self, Self::Mistake | Self::Blunder)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct StrengthSample {
    pub color: Stone,
    pub move_number: u32,
    pub coordinate: String,
    pub winrate_loss: f64,
    pub score_loss: Option<f64>,
    pub first_choice: bool,
    pub ai_rank: u32,
    pub category: StrengthMoveCategory,
    pub move_rank: MoveRank,
    pub score_equivalent_loss: f64,
    pub complexity: f64,
    pub adjusted_weight: f64,
    /// Retained internally so segment summaries use the original HumanSL features.
    #[serde(skip)]
    #[ts(skip)]
    pub human: Option<HumanMove>,
}
fn finite(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}
fn clamp(value: f64, low: f64, high: f64) -> f64 {
    if value.is_nan() {
        f64::NAN
    } else {
        value.max(low).min(high)
    }
}
fn positive(value: f64) -> f64 {
    clamp(value, 0.0, f64::INFINITY)
}

#[cfg(test)]
mod tests;

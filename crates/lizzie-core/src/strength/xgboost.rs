use serde::Deserialize;
use std::sync::LazyLock;

pub const FULL_FEATURE_NAMES: [&str; 29] = [
    "first_choice_rate",
    "top3_rate",
    "top5_rate",
    "average_ai_rank_fit",
    "excellent_rate",
    "good_move_rate",
    "non_inaccuracy_rate",
    "match_rate",
    "non_mistake_rate",
    "non_blunder_rate",
    "weighted_loss_fit",
    "average_loss_fit",
    "median_loss_fit",
    "p75_loss_fit",
    "p90_loss_fit",
    "p95_loss_fit",
    "max_loss_fit",
    "loss_stability_fit",
    "difficulty_fit",
    "opening_loss_fit",
    "middlegame_loss_fit",
    "endgame_loss_fit",
    "opening_good_move_rate",
    "middlegame_good_move_rate",
    "endgame_good_move_rate",
    "first_choice_x_difficulty",
    "good_move_x_difficulty",
    "match_x_difficulty",
    "top5_x_difficulty",
];
pub const DEFAULT_FEATURE_INDICES: [usize; 20] =
    [13, 5, 2, 19, 10, 20, 26, 3, 14, 11, 28, 8, 18, 7, 12, 23, 27, 22, 25, 0];
pub fn full_feature_index(name: &str) -> Option<usize> {
    FULL_FEATURE_NAMES.iter().position(|n| *n == name)
}

#[derive(Clone, Debug)]
pub struct Booster {
    pub feature_count: usize,
    pub feature_names: Vec<String>,
    base: f64,
    trees: Vec<Tree>,
}
#[derive(Clone, Debug, Deserialize)]
struct Tree {
    left_children: Vec<i32>,
    right_children: Vec<i32>,
    split_indices: Vec<usize>,
    split_conditions: Vec<f64>,
    default_left: Vec<u8>,
}
impl Booster {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let root: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let learner = &root["learner"];
        let param = &learner["learner_model_param"];
        let base = param["base_score"]
            .as_str()
            .ok_or("Missing base_score")?
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim()
            .parse::<f64>()
            .map_err(|e| e.to_string())?;
        let feature_count = param["num_feature"]
            .as_str()
            .ok_or("Missing num_feature")?
            .parse::<usize>()
            .map_err(|e| e.to_string())?;
        let trees: Vec<Tree> =
            serde_json::from_value(learner["gradient_booster"]["model"]["trees"].clone()).map_err(|e| e.to_string())?;
        // A malformed user booster must be unavailable rather than panic while scoring.
        for tree in &trees {
            let n = tree.left_children.len();
            if n == 0
                || tree.right_children.len() != n
                || tree.split_indices.len() != n
                || tree.split_conditions.len() != n
                || tree.default_left.len() != n
            {
                return Err("Invalid booster tree".into());
            }
            for i in 0..n {
                if tree.left_children[i] != -1
                    && (tree.left_children[i] < 0
                        || tree.right_children[i] < 0
                        || tree.left_children[i] as usize >= n
                        || tree.right_children[i] as usize >= n
                        || tree.split_indices[i] >= feature_count)
                {
                    return Err("Invalid booster tree".into());
                }
            }
        }
        let feature_names = learner["feature_names"]
            .as_array()
            .map(|a| a.iter().map(|v| v.as_str().unwrap_or_default().to_owned()).collect())
            .unwrap_or_default();
        Ok(Self {
            feature_count,
            feature_names,
            base,
            trees,
        })
    }
    pub fn predict(&self, features: &[f64]) -> Result<f64, String> {
        if features.len() < self.feature_count {
            return Err("Missing strength model or wrong feature count".into());
        }
        let mut prediction = self.base;
        for tree in &self.trees {
            let mut node = 0;
            let mut traversed = 0;
            while tree.left_children[node] != -1 {
                if traversed >= tree.left_children.len() {
                    return Err("Invalid booster tree".into());
                }
                traversed += 1;
                let value = features[tree.split_indices[node]];
                let left = if value.is_nan() {
                    tree.default_left[node] != 0
                } else {
                    value < tree.split_conditions[node]
                };
                node = if left {
                    tree.left_children[node]
                } else {
                    tree.right_children[node]
                } as usize;
            }
            prediction += tree.split_conditions[node];
        }
        Ok(prediction)
    }
    pub fn predict_uncalibrated(&self, features: &[f64], names: &[&str]) -> Result<f64, String> {
        if features.len() != self.feature_count
            || !self.feature_names.iter().map(String::as_str).eq(names.iter().copied())
        {
            return Err("Missing strength model or wrong feature count".into());
        }
        if features.iter().any(|v| v.is_infinite()) {
            return Err("Infinite feature value".into());
        }
        Ok(super::clamp(self.predict(features)?, 0.0, 12.0))
    }
}
pub(super) fn bundled() -> &'static Booster {
    &BUNDLED
}
static BUNDLED: LazyLock<Booster> = LazyLock::new(|| {
    Booster::parse(include_bytes!("../../resources/strength/xgboost20tun_booster.json"))
        .expect("bundled strength booster")
});
pub(super) fn human() -> &'static Booster {
    &HUMAN
}
static HUMAN: LazyLock<Booster> = LazyLock::new(|| {
    Booster::parse(include_bytes!("../../resources/strength/humansl40_booster.json")).expect("bundled HumanSL booster")
});
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tree_missing_and_threshold() {
        let b = Booster::parse(br#"{"learner":{"learner_model_param":{"base_score":"[2.0]","num_feature":"1"},"gradient_booster":{"model":{"trees":[{"left_children":[1,-1,-1],"right_children":[2,-1,-1],"split_indices":[0,0,0],"split_conditions":[0.5,-1,3],"default_left":[1,0,0]}]}}}}"#).unwrap();
        assert_eq!(b.predict(&[0.49]).unwrap(), 1.0);
        assert_eq!(b.predict(&[0.5]).unwrap(), 5.0);
        assert_eq!(b.predict(&[f64::NAN]).unwrap(), 1.0);
    }
}

use super::clamp;
use serde_json::Value;
use std::sync::LazyLock;
#[derive(Clone, Debug)]
pub(super) struct Calibrator {
    start: f64,
    full: f64,
    minimum: f64,
    maximum: f64,
    order: Vec<String>,
    mean: Vec<f64>,
    scale: Vec<f64>,
    intercept: f64,
    coefficients: Vec<f64>,
    match_gate: Vec<f64>,
    top5_gate: Vec<f64>,
    loss_gate: Vec<f64>,
}
impl Calibrator {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let root: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let c = &root["final_calibrator"];
        let s = &c["spec"];
        let num = |v: &Value| v.as_f64().ok_or_else(|| "Invalid calibrator number".to_owned());
        let array = |v: &Value| -> Result<Vec<f64>, String> {
            v.as_array()
                .ok_or("Invalid calibrator array")?
                .iter()
                .map(num)
                .collect()
        };
        let gate = |name: &str, fallback: &[f64]| -> Result<Vec<f64>, String> {
            if root["quality_gate_thresholds"][name].is_null() {
                Ok(fallback.to_vec())
            } else {
                array(&root["quality_gate_thresholds"][name])
            }
        };
        Ok(Self {
            start: num(&s["gate_start"])?,
            full: num(&s["gate_full"])?,
            minimum: s["correction_min"].as_f64().unwrap_or(-0.5),
            maximum: s["correction_max"].as_f64().unwrap_or(1.0),
            order: c["feature_order"]
                .as_array()
                .ok_or("Invalid calibrator feature_order")?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| "Invalid calibrator feature name".to_owned())
                })
                .collect::<Result<_, _>>()?,
            mean: array(&c["scaler_mean"])?,
            scale: array(&c["scaler_scale"])?,
            intercept: num(&c["intercept"])?,
            coefficients: array(&c["coefficients"])?,
            match_gate: gate("match_rate", &[0.52, 0.60])?,
            top5_gate: gate("top5_rate", &[0.68, 0.80])?,
            loss_gate: gate("weighted_loss_fit", &[0.32, 0.50])?,
        })
    }
    pub fn apply(&self, base: f64, f: &[f64; 29]) -> f64 {
        let n = self.coefficients.len();
        if self.order.len() != n || self.mean.len() != n || self.scale.len() != n {
            return clamp(base, -1.0, 12.0);
        }
        let mut correction = self.intercept;
        for i in 0..n {
            let value = match self.order[i].as_str() {
                "base_prediction" => base,
                "match_rate" => f[7],
                "first_choice_rate" => f[0],
                "top5_rate" => f[2],
                "weighted_loss_fit" => f[10],
                "difficulty_fit" => f[18],
                name => name
                    .strip_prefix("hinge_above_")
                    .and_then(|s| s.parse::<f64>().ok())
                    .map(|t| (base - t).max(0.0))
                    .unwrap_or(f64::NAN),
            };
            if !value.is_finite() {
                return clamp(base, -1.0, 12.0);
            }
            let scale = if self.scale[i] == 0.0 { 1.0 } else { self.scale[i] };
            correction += self.coefficients[i] * ((value - self.mean[i]) / scale);
        }
        let rank_gate = clamp((base - self.start) / (self.full - self.start).max(1e-9), 0.0, 1.0);
        let gate = |v: f64, limits: &[f64]| {
            if limits.len() < 2 {
                0.0
            } else {
                clamp((v - limits[0]) / (limits[1] - limits[0]).max(1e-9), 0.0, 1.0)
            }
        };
        let max = |a: f64, b: f64| if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) };
        let quality = max(
            gate(f[7], &self.match_gate),
            max(gate(f[2], &self.top5_gate), gate(f[10], &self.loss_gate)),
        );
        clamp(
            base + rank_gate * quality * clamp(correction, self.minimum, self.maximum),
            -1.0,
            12.0,
        )
    }
}
pub(super) fn bundled() -> &'static Calibrator {
    &BUNDLED
}
static BUNDLED: LazyLock<Calibrator> = LazyLock::new(|| {
    Calibrator::parse(include_bytes!(
        "../../resources/strength/xgboost20tun_residual_calibrator.json"
    ))
    .expect("bundled strength calibrator")
});
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gates_and_correction_cap() {
        let c = Calibrator::parse(br#"{"final_calibrator":{"spec":{"gate_start":5,"gate_full":7},"feature_order":["base_prediction"],"scaler_mean":[0],"scaler_scale":[0],"intercept":0,"coefficients":[1]}}"#).unwrap();
        assert_eq!(c.apply(4.0, &[1.0; 29]), 4.0);
        assert_eq!(c.apply(6.0, &[1.0; 29]), 6.5);
        assert_eq!(c.apply(7.0, &[0.0; 29]), 7.0);
    }
}

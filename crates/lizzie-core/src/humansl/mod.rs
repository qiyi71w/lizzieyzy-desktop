//! HumanSL protocol and human-like move selection. No application or process state.
pub mod protocol;
use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub vertex: String,
    pub probability: f64,
}

pub fn candidate_pool(moves: &[Candidate]) -> Vec<Candidate> {
    let mut sorted: Vec<_> = moves
        .iter()
        .filter(|m| !m.vertex.trim().is_empty() && m.probability.is_finite() && m.probability > 0.0)
        .cloned()
        .collect();
    sorted.sort_by(|a, b| b.probability.total_cmp(&a.probability));
    let total: f64 = sorted.iter().map(|m| m.probability).sum();
    let Some(first) = sorted.first() else { return sorted };
    let top = first.probability;
    if !total.is_finite() {
        sorted.truncate(1);
        return sorted;
    }
    let mut mass = 0.0;
    let mut count = 0;
    for m in &sorted {
        if count == 12 || (count > 0 && m.probability < top * 0.01) {
            break;
        }
        count += 1;
        mass += m.probability;
        if mass / total >= 0.95 {
            break;
        }
    }
    sorted.truncate(count);
    sorted
}

pub fn temperature(move_number: u32) -> f64 {
    match move_number {
        0..=60 => 1.25,
        61..=160 => 1.05,
        _ => 0.90,
    }
}

fn band(profile: &str) -> usize {
    let profile = profile.to_ascii_lowercase();
    if profile.starts_with("proyear_") {
        return 0;
    }
    let rank = profile
        .strip_prefix("rank_")
        .and_then(|s| s.get(..s.len().saturating_sub(1)))
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(10);
    if profile.ends_with('d') {
        match rank {
            7.. => 0,
            4..=6 => 1,
            _ => 2,
        }
    } else {
        match rank {
            0..=5 => 3,
            6..=10 => 4,
            _ => 5,
        }
    }
}

struct Quality<'a> {
    values: Vec<(&'a str, f64)>,
    best: f64,
    limit: f64,
    fallback: Option<&'a str>,
}
impl<'a> Quality<'a> {
    fn new(infos: &'a [Value], profile: &str) -> Self {
        let fallback = infos
            .iter()
            .min_by_key(|m| m["order"].as_u64().unwrap_or(u64::MAX))
            .and_then(|m| m["move"].as_str());
        for (field, limits) in [
            ("utility", [0.32, 0.45, 0.65, 0.85, 1.15, 1.80]),
            ("scoreLead", [3.0, 4.0, 5.5, 7.0, 10.0, 15.0]),
            ("winrate", [0.10, 0.14, 0.20, 0.25, 0.34, 0.48]),
        ] {
            let values: Vec<_> = infos
                .iter()
                .filter_map(|m| {
                    let vertex = m["move"].as_str()?;
                    let mut value = m[field]
                        .as_f64()
                        .or_else(|| (field == "scoreLead").then(|| m["scoreMean"].as_f64()).flatten())?;
                    if field == "winrate" && value > 1.0 {
                        value /= 100.0;
                    }
                    value.is_finite().then_some((vertex, value))
                })
                .collect();
            if !values.is_empty() {
                let &(vertex, best) = values.iter().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
                return Self {
                    values,
                    best,
                    limit: limits[band(profile)],
                    fallback: Some(vertex),
                };
            }
        }
        Self {
            values: Vec::new(),
            best: 0.0,
            limit: 1.0,
            fallback,
        }
    }
    fn loss(&self, vertex: &str) -> Option<f64> {
        self.values
            .iter()
            .find(|(v, _)| v.eq_ignore_ascii_case(vertex))
            .map(|(_, v)| (self.best - v).max(0.0))
    }
}

pub fn needs_deep_verification(moves: &[Candidate], infos: &[Value], profile: &str) -> bool {
    let pool = candidate_pool(moves);
    if pool.is_empty() {
        return false;
    }
    let q = Quality::new(infos, profile);
    pool.iter()
        .any(|m| q.loss(&m.vertex).is_none_or(|loss| loss > q.limit * 0.55))
}

pub fn select(moves: &[Candidate], infos: &[Value], move_number: u32, profile: &str, random: f64) -> Option<String> {
    let pool = candidate_pool(moves);
    if pool.is_empty() {
        return None;
    }
    let q = Quality::new(infos, profile);
    let weighted: Vec<_> = pool
        .iter()
        .filter_map(|m| {
            let loss = q.loss(&m.vertex)?;
            if loss > q.limit {
                return None;
            }
            let weight = m.probability.powf(1.0 / temperature(move_number)) * (-loss / q.limit).exp();
            (weight.is_finite() && weight > 0.0).then_some((&m.vertex, weight))
        })
        .collect();
    let mut target = random.clamp(0.0, 0.999999999999) * weighted.iter().map(|(_, w)| w).sum::<f64>();
    for (vertex, weight) in &weighted {
        target -= weight;
        if target < 0.0 {
            return Some((*vertex).clone());
        }
    }
    weighted
        .last()
        .map(|(v, _)| (*v).clone())
        .or_else(|| q.fallback.map(str::to_string))
}

pub fn adaptive_visits(completed: u32, elapsed: std::time::Duration, remaining: std::time::Duration) -> u32 {
    let usable = remaining.as_secs_f64() - 0.450;
    if usable < 0.200 || elapsed.is_zero() {
        return completed;
    }
    let estimate = f64::from(completed.max(1)) * usable / elapsed.as_secs_f64() * 0.85;
    let minimum = completed.saturating_add(16.max(completed / 4)).min(4096);
    if estimate < f64::from(minimum) {
        completed
    } else {
        (estimate as u32).min(4096)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn mass_cap_quality_and_temperature_follow_java() {
        let moves: Vec<_> = (0..20)
            .map(|i| Candidate {
                vertex: format!("A{i}"),
                probability: 1.0,
            })
            .collect();
        assert_eq!(candidate_pool(&moves).len(), 12);
        let moves = vec![
            Candidate {
                vertex: "D4".into(),
                probability: 0.96,
            },
            Candidate {
                vertex: "Q16".into(),
                probability: 0.04,
            },
        ];
        assert_eq!(candidate_pool(&moves).len(), 1);
        let infos = vec![json!({"move":"D4","utility":0.0}), json!({"move":"Q16","utility":-0.5})];
        let moves = vec![
            Candidate {
                vertex: "D4".into(),
                probability: 0.5,
            },
            Candidate {
                vertex: "Q16".into(),
                probability: 0.5,
            },
        ];
        assert_eq!(select(&moves, &infos, 1, "rank_9d", 0.99).as_deref(), Some("D4"));
        assert_eq!(select(&moves, &infos, 1, "rank_3d", 0.99).as_deref(), Some("Q16"));
        assert_eq!(
            [temperature(60), temperature(61), temperature(160), temperature(161)],
            [1.25, 1.05, 1.05, 0.90]
        );
    }
    #[test]
    fn quality_guard_uses_each_java_strength_band_and_metric() {
        let profiles = ["rank_9d", "rank_5d", "rank_3d", "rank_3k", "rank_8k", "rank_15k"];
        for (field, limits) in [
            ("utility", [0.32, 0.45, 0.65, 0.85, 1.15, 1.80]),
            ("scoreLead", [3.0, 4.0, 5.5, 7.0, 10.0, 15.0]),
            ("winrate", [0.10, 0.14, 0.20, 0.25, 0.34, 0.48]),
        ] {
            for (profile, limit) in profiles.iter().zip(limits) {
                let base = if field == "winrate" { 0.8 } else { 0.0 };
                let infos = vec![
                    json!({"move":"D4",(field):base}),
                    json!({"move":"Q16",(field):base-limit*1.01}),
                ];
                let moves = vec![
                    Candidate {
                        vertex: "D4".into(),
                        probability: 0.5,
                    },
                    Candidate {
                        vertex: "Q16".into(),
                        probability: 0.5,
                    },
                ];
                assert_eq!(
                    select(&moves, &infos, 100, profile, 0.999).as_deref(),
                    Some("D4"),
                    "{field} {profile}"
                );
                assert!(needs_deep_verification(&moves, &infos, profile));
            }
        }
    }
    #[test]
    fn adaptive_work_respects_return_reserve_and_visit_ceiling() {
        use std::time::Duration;
        assert_eq!(
            adaptive_visits(64, Duration::from_secs(1), Duration::from_millis(500)),
            64
        );
        assert_eq!(
            adaptive_visits(64, Duration::from_millis(1), Duration::from_secs(10)),
            4096
        );
        assert_eq!(adaptive_visits(64, Duration::from_secs(10), Duration::from_secs(1)), 64);
    }
}

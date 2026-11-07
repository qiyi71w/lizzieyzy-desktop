use super::{clamp, xgboost};
use serde_json::Value;
pub const HUMAN_SHA: &str = "637746e44f0efe00ad1245a50aa9bbf0716efe364c43965ead97bd6835d84ab5";
pub const PROFILES: [&str; 3] = ["rank_1d", "rank_5d", "rank_9d"];
const NAMES: [&str; 40] = [
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
    "median_loss_fit",
    "p75_loss_fit",
    "difficulty_fit",
    "opening_good_move_rate",
    "middlegame_good_move_rate",
    "endgame_good_move_rate",
    "first_choice_x_difficulty",
    "good_move_x_difficulty",
    "match_x_difficulty",
    "top5_x_difficulty",
    "capped_mean_loss_3",
    "capped_mean_loss_5",
    "capped_mean_loss_10",
    "mean_log_loss",
    "top3_loss_share",
    "lost_position_fraction",
    "reversal_fraction",
    "adjacent_root_fraction",
    "rank_1d_mean_surprisal",
    "rank_1d_difficulty",
    "rank_1d_coverage_mean",
    "rank_1d_reliable_fraction",
    "rank_5d_mean_surprisal",
    "rank_5d_difficulty",
    "rank_5d_coverage_mean",
    "rank_5d_reliable_fraction",
    "rank_9d_mean_surprisal",
    "rank_9d_difficulty",
    "rank_9d_coverage_mean",
    "rank_9d_reliable_fraction",
];
#[derive(Clone, Debug, PartialEq)]
pub struct HumanMove {
    pub number: u32,
    pub rank: u32,
    pub color: String,
    pub loss: f64,
    pub complexity: f64,
    pub actor_winrate: f64,
    pub delta: f64,
    pub adjacent: bool,
    pub reversal: bool,
    pub human: [[f64; 4]; 3],
}
/// Java DoubleStream's compensated summation, including its infinite-value fallback.
fn java_sum(values: &[f64]) -> f64 {
    let mut sum = 0.0;
    let mut compensation = 0.0;
    let mut simple = 0.0;
    for &v in values {
        let corrected = v - compensation;
        let next = sum + corrected;
        compensation = (next - sum) - corrected;
        sum = next;
        simple += v;
    }
    let final_sum = sum - compensation;
    if final_sum.is_nan() && simple.is_infinite() {
        simple
    } else {
        final_sum
    }
}
pub(super) fn java_mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        f64::NAN
    } else {
        java_sum(values) / values.len() as f64
    }
}
fn rounded(v: f64, places: i32) -> f64 {
    let scale = 10f64.powi(places);
    (v * scale).round_ties_even() / scale
}
fn rate(moves: &[HumanMove], f: impl Fn(&HumanMove) -> bool) -> f64 {
    moves.iter().filter(|m| f(m)).count() as f64 / moves.len() as f64
}
fn quantile(s: &[f64], p: f64) -> f64 {
    let index = (s.len() - 1) as f64 * p;
    let left = index as usize;
    let right = (left + 1).min(s.len() - 1);
    s[left] + (s[right] - s[left]) * (index - left as f64)
}
pub fn human_features(moves: &[HumanMove]) -> [f64; 40] {
    if moves.is_empty() {
        return [f64::NAN; 40];
    }
    let first = rate(moves, |m| m.rank == 0);
    let top3 = rate(moves, |m| m.rank < 3);
    let top5 = rate(moves, |m| m.rank < 5);
    let excellent = rate(moves, |m| m.loss < 0.2);
    let good = rate(moves, |m| m.loss < 1.2);
    let mistake = rate(moves, |m| m.loss >= 4.0);
    let blunder = rate(moves, |m| m.loss >= 10.0);
    let matched = rounded(0.45 * first + 0.45 * good + 0.10 * (1.0 - mistake), 4);
    let difficulty = clamp(
        (rounded(
            100.0 * java_mean(&moves.iter().map(|m| m.complexity).collect::<Vec<_>>()),
            1,
        ) - 25.0)
            / 35.0,
        0.0,
        1.0,
    );
    let losses: Vec<_> = moves.iter().map(|m| m.loss).collect();
    let mut sorted = losses.clone();
    sorted.sort_by(f64::total_cmp);
    let first = rounded(first, 4);
    let top3 = rounded(top3, 4);
    let top5 = rounded(top5, 4);
    let good = rounded(good, 4);
    let rank = rounded(
        java_mean(
            &moves
                .iter()
                .map(|m| {
                    if m.rank >= 999 {
                        10.0
                    } else {
                        (m.rank as f64 + 1.0).min(10.0)
                    }
                })
                .collect::<Vec<_>>(),
        ),
        3,
    );
    let phase = |first, last| {
        let phase: Vec<_> = moves.iter().filter(|m| m.number >= first && m.number <= last).collect();
        if phase.is_empty() {
            good
        } else {
            rounded(
                phase.iter().filter(|m| m.loss < 1.2).count() as f64 / phase.len() as f64,
                4,
            )
        }
    };
    let mut result = [0.0; 40];
    result[..20].copy_from_slice(&[
        first,
        top3,
        top5,
        1.0 / (1.0 + rank),
        rounded(excellent, 4),
        good,
        1.0 - rounded(rate(moves, |m| m.loss >= 1.2 && m.loss < 4.0), 4),
        matched,
        1.0 - rounded(mistake, 4),
        1.0 - rounded(blunder, 4),
        1.0 / (1.0 + rounded(quantile(&sorted, 0.5), 3).min(50.0)),
        1.0 / (1.0 + rounded(quantile(&sorted, 0.75), 3).min(50.0)),
        difficulty,
        phase(1, 60),
        phase(61, 160),
        phase(161, 210),
        first * difficulty,
        good * difficulty,
        matched * difficulty,
        top5 * difficulty,
    ]);
    for (j, cap) in [3.0, 5.0, 10.0].iter().enumerate() {
        result[20 + j] = java_mean(&losses.iter().map(|v| v.min(*cap)).collect::<Vec<_>>());
    }
    result[23] = java_mean(&losses.iter().map(|v| v.ln_1p()).collect::<Vec<_>>());
    let total = java_sum(&losses);
    result[24] = if total > 0.0 {
        java_sum(&sorted[sorted.len().saturating_sub(3)..]) / total
    } else {
        0.0
    };
    result[25] = rate(moves, |m| m.actor_winrate < 0.05);
    result[26] = rate(moves, |m| m.reversal);
    result[27] = rate(moves, |m| m.adjacent);
    for p in 0..3 {
        for j in 0..4 {
            result[28 + 4 * p + j] = java_mean(
                &moves
                    .iter()
                    .map(|m| m.human[p][j])
                    .filter(|v| v.is_finite())
                    .collect::<Vec<_>>(),
            );
        }
    }
    result
}
pub(super) fn predict(moves: &[HumanMove]) -> Result<f64, String> {
    if moves.len() < 6 {
        return Ok(f64::NAN);
    }
    xgboost::human().predict_uncalibrated(&human_features(moves), &NAMES)
}
#[cfg(test)]
pub(super) fn predict_vector(features: &[f64]) -> Result<f64, String> {
    xgboost::human().predict_uncalibrated(features, &NAMES)
}
fn score(v: &Value) -> Result<f64, String> {
    v.get("scoreMean")
        .or_else(|| v.get("scoreLead"))
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite())
        .ok_or_else(|| "Nonfinite candidate score".into())
}
fn candidates(v: &Value) -> Result<Vec<&Value>, String> {
    if let Some(e) = v.get("error") {
        return Err(e.as_str().unwrap_or_default().into());
    }
    let mut result: Vec<_> = v["moveInfos"].as_array().ok_or("No candidates")?.iter().collect();
    result.sort_by_key(|m| m["order"].as_i64().unwrap_or(0));
    Ok(result)
}
fn probability(policy: &Value, coordinate: &str, size: usize) -> Result<f64, String> {
    let array = policy
        .as_array()
        .filter(|a| a.len() == size * size + 1)
        .ok_or("Wrong human policy size")?;
    let index = if coordinate.eq_ignore_ascii_case("pass") {
        size * size
    } else {
        let name = coordinate.to_uppercase();
        let x = name
            .chars()
            .next()
            .and_then(|c| "ABCDEFGHJKLMNOPQRST".find(c))
            .ok_or("Invalid coordinate")?;
        let row = name
            .get(1..)
            .ok_or("Invalid coordinate")?
            .parse::<usize>()
            .map_err(|_| "Invalid coordinate")?;
        if x >= size || row == 0 || row > size {
            return Err("Invalid coordinate".into());
        }
        (size - row) * size + x
    };
    array[index]
        .as_f64()
        .filter(|v| v.is_finite() && *v >= 0.0 && *v <= 1.0)
        .ok_or_else(|| format!("Missing/invalid human probability for {coordinate}"))
}
fn human(candidates: &[&Value], policy: &Value, played: &str, size: usize) -> Result<[f64; 4], String> {
    let mut mass = 0.0;
    let mut good = 0.0;
    let top = score(candidates[0])?;
    for c in candidates {
        let p = probability(policy, c["move"].as_str().ok_or("Invalid coordinate")?, size)?;
        mass += p;
        if (top - score(c)?).max(0.0) <= 1.2 {
            good += p;
        }
    }
    mass = clamp(mass, 0.0, 1.0);
    let lower = (mass - good).max(0.0);
    let upper = (1.0 - good).min(1.0);
    Ok([
        -probability(policy, played, size)?.max(1e-9).ln(),
        if mass >= 0.9 { (lower + upper) / 2.0 } else { f64::NAN },
        mass,
        if mass >= 0.9 { 1.0 } else { 0.0 },
    ])
}
pub(super) fn extract(
    moves: &Value,
    base: &Value,
    policies: &Value,
    size: usize,
    allow_missing: bool,
) -> Result<Vec<HumanMove>, String> {
    let moves = moves.as_array().ok_or("Invalid moves")?;
    let base = base.as_array().ok_or("Invalid base")?;
    if size != 19 || moves.len() > 210 || base.len() != moves.len() + 1 {
        return Err("Expected 19x19, at most 210 moves and all base positions".into());
    }
    for profile in PROFILES {
        if policies[profile].as_array().map(Vec::len) != Some(base.len()) {
            return Err(format!("Incomplete HumanSL profile: {profile}"));
        }
    }
    let mut all = Vec::new();
    for turn in 0..moves.len() {
        if allow_missing
            && (!base[turn].is_object()
                || !base[turn + 1].is_object()
                || base[turn]["moveInfos"].as_array().is_none_or(Vec::is_empty))
        {
            continue;
        }
        let before = &base[turn];
        let after = &base[turn + 1];
        let candidates = candidates(before)?;
        if candidates.is_empty() {
            return Err(format!("No candidates at turn {turn}"));
        }
        let color = moves[turn][0].as_str().ok_or("Invalid move color")?;
        if color != "B" && color != "W" {
            return Err("Invalid move color".into());
        }
        let played = moves[turn][1].as_str().ok_or("Invalid coordinate")?;
        let rank = candidates
            .iter()
            .position(|c| c["move"].as_str().is_some_and(|s| s.eq_ignore_ascii_case(played)));
        let top = score(candidates[0])?;
        let root_before = score(&before["rootInfo"])?;
        let root_after = score(&after["rootInfo"])?;
        let loss = match rank {
            Some(i) => (top - score(candidates[i])?).max(0.0),
            None => (root_before + root_after).max(0.0),
        };
        let black_before = root_before * if color == "B" { 1.0 } else { -1.0 };
        let next_color = if turn + 1 < moves.len() {
            moves[turn + 1][0].as_str().unwrap_or_default()
        } else if color == "B" {
            "W"
        } else {
            "B"
        };
        let delta = root_after * if next_color == "B" { 1.0 } else { -1.0 } - black_before;
        let mut priors = 0.0;
        let mut loss_sum = 0.0;
        for c in &candidates {
            let p = c["prior"].as_f64().unwrap_or(0.0).max(0.0);
            if c["order"].as_i64().unwrap_or(0) < 999 && p > 0.0 {
                priors += p;
                loss_sum += p * (top - score(c)?).max(0.0);
            }
        }
        let complexity = if candidates.len() < 2 {
            0.0
        } else {
            clamp(
                if priors > 0.0 {
                    loss_sum / priors
                } else {
                    (top - score(candidates[1])?).max(0.0) / 1.2
                },
                0.0,
                1.0,
            )
        };
        let mut h = [[0.0; 4]; 3];
        for p in 0..3 {
            h[p] = human(&candidates, &policies[PROFILES[p]][turn], played, size)?;
        }
        all.push(HumanMove {
            number: turn as u32 + 1,
            rank: rank.map(|i| i as u32).unwrap_or(999),
            color: color.into(),
            loss,
            complexity,
            actor_winrate: before["rootInfo"]["winrate"].as_f64().ok_or("Invalid winrate")?,
            delta,
            adjacent: rank.is_none(),
            reversal: false,
            human: h,
        });
    }
    for end in 3..all.len() {
        let mut start = end.saturating_sub(7);
        while start < end && all[start].number < all[end].number.saturating_sub(7) {
            start += 1;
        }
        if (start + 1..=end).any(|i| all[i].number != all[i - 1].number + 1) {
            continue;
        }
        let large: Vec<_> = all[start..=end]
            .iter()
            .map(|m| m.delta)
            .filter(|d| d.abs() >= 8.0)
            .collect();
        if large.windows(2).filter(|p| p[0] * p[1] < 0.0).count() >= 3 {
            for m in &mut all[start..=end] {
                m.reversal = true;
            }
        }
    }
    Ok(all)
}

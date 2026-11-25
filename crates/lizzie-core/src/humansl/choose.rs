//! How a level picks the AI's move from KataGo's answer (Java `analysis/HumanPolicySampler`,
//! `HumanSlChosenMove`).
//!
//! A policy level samples the raw HumanSL policy, as KataGo's `gtp_human5k_example.cfg` plays:
//! moves with at least 1% of the policy keep their human odds, rarer ones are damped by a
//! temperature that falls from 0.85 to 0.70 over the game.
//!
//! A search level searches once with its settings, and the move is the one KataGo's genmove would
//! choose from that search (`Search::getPlaySelectionValues` and `getChosenMoveLoc` of KataGo 1.18
//! with `humanSLChosenMoveProp` 1), since the analysis engine never chooses a move itself:
//! 1. every legal move gets its HumanSL odds times `exp((utility - best utility) / lambda)`, the
//!    PiKL shift; moves the search did not reach count at the average utility of the ones it did;
//! 2. with `humanSLChosenMoveIgnorePass`, passing keeps the share the search gives it and the other
//!    moves share the rest;
//! 3. the level's temperature, falling with the move number, damps moves rarer than
//!    `chosenMoveTemperatureOnlyBelowProb`, and one move is drawn.
//!
//! Symmetric copies the analysis engine reports (`isSymmetryOf`) are left out as genmove leaves
//! them out; a move without visits counts as not searched.
use super::{Candidate, PlaySettings};
use serde_json::Value;

const RARE_MOVE_PROBABILITY: f64 = 0.01;
const POLICY_EARLY_TEMPERATURE: f64 = 0.85;
const POLICY_LATE_TEMPERATURE: f64 = 0.70;
const POLICY_TEMPERATURE_HALFLIFE: f64 = 80.0;

/// A policy level's move: one of `moves` (the legal moves with their HumanSL odds), drawn with
/// `random` uniform in [0, 1). `None` when no move has any weight.
pub fn policy_move(moves: &[Candidate], move_number: u32, width: u16, height: u16, random: f64) -> Option<&str> {
    let temperature = interpolate_early(
        move_number,
        width,
        height,
        POLICY_TEMPERATURE_HALFLIFE,
        POLICY_EARLY_TEMPERATURE,
        POLICY_LATE_TEMPERATURE,
    );
    let odds: Vec<f64> = moves.iter().map(|m| m.probability).collect();
    let index = sample_index(
        &temperature_weights(&odds, temperature.max(1.0e-4), RARE_MOVE_PROBABILITY),
        random,
    )?;
    Some(&moves[index].vertex)
}

/// The share of the policy on its most popular move; `None` when the policy is empty.
pub fn top_share(moves: &[Candidate]) -> Option<f64> {
    let (max, sum) = moves
        .iter()
        .map(|m| m.probability)
        .filter(|p| p.is_finite() && *p > 0.0)
        .fold((0.0_f64, 0.0_f64), |(max, sum), p| (max.max(p), sum + p));
    (sum > 0.0 && sum.is_finite()).then(|| max / sum)
}

/// A search level's move: the move genmove would choose from `move_infos` (the analysis
/// response's, utilities from the side to move) with `human_moves` the legal moves and their
/// HumanSL odds (a pass only when passing is allowed). `None` when no move has any weight.
pub fn searched_move(
    move_infos: &[Value],
    human_moves: &[Candidate],
    play: &PlaySettings,
    move_number: u32,
    (width, height): (u16, u16),
    allow_pass: bool,
    random: f64,
) -> Option<String> {
    let (moves, values) = chosen_move_weights(move_infos, human_moves, play, allow_pass);
    let temperature = interpolate_early(
        move_number,
        width,
        height,
        play.temperature_halflife(),
        play.temperature_early(),
        play.temperature(),
    );
    let index = sample_index(
        &temperature_weights(&values, temperature, play.temperature_only_below_prob),
        random,
    )?;
    moves.into_iter().nth(index)
}

fn normalize(vertex: &str) -> String {
    let trimmed = vertex.trim();
    if trimmed.eq_ignore_ascii_case("pass") {
        "pass".into()
    } else {
        trimmed.to_ascii_uppercase()
    }
}

/// Steps 1 and 2: the PiKL-shifted human odds, with the pass at its search share.
fn chosen_move_weights(
    move_infos: &[Value],
    human_moves: &[Candidate],
    play: &PlaySettings,
    allow_pass: bool,
) -> (Vec<String>, Vec<f64>) {
    let mut human_odds: Vec<(String, f64)> = Vec::new();
    for m in human_moves {
        let vertex = normalize(&m.vertex);
        let odds = m.probability.max(0.0);
        match human_odds.iter_mut().find(|(v, _)| *v == vertex) {
            Some((_, sum)) => *sum += odds,
            None => human_odds.push((vertex, odds)),
        }
    }
    let mut moves: Vec<String> = Vec::new();
    let mut utilities: Vec<f64> = Vec::new();
    let mut selection: Vec<f64> = Vec::new();
    let mut symmetry_copies: Vec<String> = Vec::new();
    let mut utility_sum = 0.0;
    let mut utility_max = f64::NEG_INFINITY;
    for info in move_infos {
        let vertex = normalize(info["move"].as_str().unwrap_or(""));
        if vertex.is_empty() {
            continue;
        }
        if info.get("isSymmetryOf").is_some() {
            symmetry_copies.push(vertex);
            continue;
        }
        let utility = info["utility"].as_f64().unwrap_or(f64::NAN);
        if info["visits"].as_f64().unwrap_or(0.0) <= 0.0 || !utility.is_finite() || moves.contains(&vertex) {
            continue;
        }
        if !human_odds.iter().any(|(v, _)| *v == vertex) {
            human_odds.push((vertex.clone(), info["humanPrior"].as_f64().unwrap_or(0.0).max(0.0)));
        }
        moves.push(vertex);
        utilities.push(utility);
        selection.push(info["playSelectionValue"].as_f64().unwrap_or(0.0).max(0.0));
        utility_sum += utility;
        utility_max = utility_max.max(utility);
    }
    let searched = moves.len();
    let utility_average = if searched == 0 {
        0.0
    } else {
        utility_sum / searched as f64
    };
    utility_max = utility_max.max(utility_average);
    for m in human_moves {
        let vertex = normalize(&m.vertex);
        if !symmetry_copies.contains(&vertex) && !moves.contains(&vertex) {
            moves.push(vertex);
        }
    }

    let lambda = play.pikl_lambda.max(1.0e-9);
    let mut values = vec![0.0; moves.len()];
    let mut shifted_sum = 0.0;
    for (i, vertex) in moves.iter().enumerate() {
        if !allow_pass && vertex == "pass" {
            continue;
        }
        let utility = if i < searched { utilities[i] } else { utility_average };
        let odds = human_odds.iter().find(|(v, _)| v == vertex).map_or(0.0, |(_, p)| *p);
        values[i] = odds * ((utility - utility_max) / lambda).exp();
        shifted_sum += values[i];
    }
    if !(shifted_sum > 0.0) || !shifted_sum.is_finite() {
        // No human odds left: KataGo keeps the search's own selection values.
        values = vec![0.0; moves.len()];
        for i in 0..searched {
            values[i] = if allow_pass || moves[i] != "pass" {
                selection[i]
            } else {
                0.0
            };
        }
    } else if play.ignore_pass() {
        keep_search_share_of_pass(&moves, &mut values, &selection, allow_pass);
    }
    (moves, values)
}

/// `humanSLChosenMoveIgnorePass`: the pass keeps its share of the search's selection values,
/// whatever its human odds, and the shifted human odds of the other moves fill the rest.
fn keep_search_share_of_pass(moves: &[String], values: &mut [f64], selection: &[f64], allow_pass: bool) {
    let selection_sum: f64 = selection.iter().sum();
    let non_pass_selection: f64 = selection
        .iter()
        .zip(moves)
        .filter(|(_, v)| *v != "pass")
        .map(|(s, _)| s)
        .sum();
    let non_pass_shifted: f64 = values
        .iter()
        .zip(moves)
        .filter(|(_, v)| *v != "pass")
        .map(|(s, _)| s)
        .sum();
    if !(selection_sum > 0.0) || !(non_pass_shifted > 0.0) {
        return;
    }
    for (value, vertex) in values.iter_mut().zip(moves) {
        *value = if vertex == "pass" {
            if allow_pass {
                (selection_sum - non_pass_selection) / selection_sum
            } else {
                0.0
            }
        } else {
            *value / non_pass_shifted * non_pass_selection / selection_sum
        };
    }
}

/// KataGo's `interpolateEarly`: the value halves its distance to `late` every halflife of moves,
/// counted as on a 19x19 board.
fn interpolate_early(move_number: u32, width: u16, height: u16, halflife: f64, early: f64, late: f64) -> f64 {
    let area = f64::from(width.max(1)) * f64::from(height.max(1));
    let halflives = f64::from(move_number) / halflife.max(1.0e-9) * 19.0 / area.sqrt();
    late + (early - late) * 0.5_f64.powf(halflives)
}

/// KataGo's `chooseIndexWithTemperature` before the draw: weights relative to the largest value,
/// where values below `only_below_prob` of the total are sharpened by the temperature and larger
/// ones keep their odds. Near zero temperature with no threshold it is the largest value.
fn temperature_weights(values: &[f64], temperature: f64, only_below_prob: f64) -> Vec<f64> {
    let mut weights = vec![0.0; values.len()];
    let mut max = 0.0;
    let mut sum = 0.0;
    let mut best = None;
    for (i, &value) in values.iter().enumerate() {
        if value > 0.0 && value.is_finite() {
            sum += value;
            if value > max {
                max = value;
                best = Some(i);
            }
        }
    }
    let Some(best) = best.filter(|_| sum.is_finite()) else {
        return weights;
    };
    if temperature <= 1.0e-4 && only_below_prob >= 1.0 {
        weights[best] = 1.0;
        return weights;
    }
    let threshold = (only_below_prob.max(1.0e-50).ln() + (sum / max).ln()).min(0.0);
    for (weight, &value) in weights.iter_mut().zip(values) {
        if !(value > 0.0) || !value.is_finite() {
            continue;
        }
        let relative = (value / max).ln();
        let damped = if relative > threshold {
            relative
        } else {
            (relative - threshold) / temperature + threshold
        };
        *weight = damped.exp();
    }
    weights
}

/// Draws an index by weight; `random` is uniform in [0, 1).
fn sample_index(weights: &[f64], random: f64) -> Option<usize> {
    let valid = |w: f64| w > 0.0 && w.is_finite();
    let total: f64 = weights.iter().copied().filter(|&w| valid(w)).sum();
    if !(total > 0.0) || !total.is_finite() {
        return None;
    }
    let target = random.clamp(0.0, 0.999_999_999_999) * total;
    let mut cumulative = 0.0;
    let mut last = None;
    for (i, &w) in weights.iter().enumerate() {
        if !valid(w) {
            continue;
        }
        last = Some(i);
        cumulative += w;
        if target < cumulative {
            return Some(i);
        }
    }
    last
}

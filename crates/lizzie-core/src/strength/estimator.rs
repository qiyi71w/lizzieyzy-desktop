use super::*;
use crate::{
    display::{self, MoveRank},
    record::Stone,
    settings::DisplaySettings,
};
use std::path::Path;

pub fn estimate(
    input: &MainLineInput,
    model_id: &str,
    models_dir: &Path,
    settings: &DisplaySettings,
) -> StrengthReport {
    estimate_with_model(input, &effective_model(model_id, models_dir), settings)
}
pub fn estimate_with_model(input: &MainLineInput, model: &StrengthModel, settings: &DisplaySettings) -> StrengthReport {
    let mut black = Vec::new();
    let mut white = Vec::new();
    for pair in input.positions.windows(2) {
        if let Some(sample) = sample(&pair[0], &pair[1], settings) {
            if sample.color == Stone::Black {
                black.push(sample);
            } else {
                white.push(sample);
            }
        }
    }
    assemble(black, white, model)
}
pub(super) fn assemble(
    black: Vec<StrengthSample>,
    white: Vec<StrengthSample>,
    model: &StrengthModel,
) -> StrengthReport {
    // Java Accumulator.merge concatenates black then white, even though graphs sort by move.
    let mut all = black.clone();
    all.extend(white.iter().cloned());
    let overall = summarize_samples(&all, model);
    StrengthReport {
        model: model.view(),
        black: summarize_samples(&black, model),
        white: summarize_samples(&white, model),
        enough_data: overall.summary.sample_count >= 6,
        overall,
    }
}
fn candidate_score(c: &CandidateInput) -> Option<f64> {
    if c.kata {
        c.score_lead.filter(|v| v.is_finite())
    } else {
        None
    }
}
fn candidate_winrate(c: &CandidateInput) -> Option<f64> {
    (c.winrate.is_finite() && (c.winrate > 0.0 || c.visits > 0 || c.kata)).then_some(c.winrate)
}
fn candidate_loss(top: &CandidateInput, actual: &CandidateInput) -> Option<f64> {
    match (candidate_score(top), candidate_score(actual)) {
        (Some(t), Some(a)) => Some(t - a),
        _ => candidate_winrate(top)
            .zip(candidate_winrate(actual))
            .map(|(t, a)| (t - a) / 6.0),
    }
}
fn complexity(a: &AnalysisInput) -> f64 {
    let Some(top) = a.candidates.first() else { return 0.0 };
    let mut sum = 0.0;
    let mut priors = 0.0;
    for (i, c) in a.candidates.iter().enumerate() {
        let order = if c.order > 0 { c.order as usize } else { i };
        if order >= 999 {
            continue;
        }
        let prior = if c.prior.is_finite() { c.prior.max(0.0) } else { 0.0 };
        if prior <= 0.0 {
            continue;
        }
        if let Some(loss) = candidate_loss(top, c) {
            sum += positive(loss) * prior;
            priors += prior;
        }
    }
    if priors > 0.0 {
        return clamp(sum / priors, 0.0, 1.0);
    }
    a.candidates
        .get(1)
        .and_then(|c| candidate_loss(top, c))
        .map(|loss| clamp(positive(loss) / 1.2, 0.0, 1.0))
        .unwrap_or(0.0)
}
pub(super) fn category(loss: f64) -> StrengthMoveCategory {
    let loss = positive(loss);
    if loss < 0.2 {
        StrengthMoveCategory::Excellent
    } else if loss < 0.6 {
        StrengthMoveCategory::Great
    } else if loss < 1.2 {
        StrengthMoveCategory::Good
    } else if loss < 4.0 {
        StrengthMoveCategory::Inaccuracy
    } else if loss < 10.0 {
        StrengthMoveCategory::Mistake
    } else {
        StrengthMoveCategory::Blunder
    }
}
pub(super) fn weight(complexity: f64, loss: f64) -> f64 {
    clamp(complexity.max(positive(loss) / 4.0), 0.05, 1.0)
}
fn sample(previous: &PositionInput, current: &PositionInput, settings: &DisplaySettings) -> Option<StrengthSample> {
    let action = current.action.as_ref()?;
    if !action.ordinary {
        return None;
    }
    let before = previous.analysis.as_ref()?;
    let after = current.analysis.as_ref()?;
    if !before.has_payload || !after.has_payload {
        return None;
    }
    let same = before.to_play == after.to_play;
    let fallback_win = after.winrate - if same { before.winrate } else { 100.0 - before.winrate };
    let fallback_score = if before.kata || after.kata {
        before
            .score_lead
            .zip(after.score_lead)
            .filter(|(b, a)| b.is_finite() && a.is_finite())
            .map(|(b, a)| a - if same { b } else { -b })
    } else {
        None
    };
    let matched = before
        .candidates
        .iter()
        .position(|c| c.coordinate.eq_ignore_ascii_case(&action.coordinate));
    let mut win = fallback_win;
    let mut score = fallback_score;
    if let Some((top, actual)) = before.candidates.first().zip(matched.map(|i| &before.candidates[i])) {
        let candidate_score = candidate_score(top).zip(candidate_score(actual)).map(|(t, a)| t - a);
        let candidate_win = candidate_winrate(top)
            .zip(candidate_winrate(actual))
            .map(|(t, a)| t - a);
        if candidate_score.is_some() || candidate_win.is_some() {
            win = candidate_win.unwrap_or(fallback_win);
            score = candidate_score;
        }
    }
    let equivalent = positive(score.map(positive).unwrap_or_else(|| positive(win) / 6.0));
    let complexity = complexity(before);
    Some(StrengthSample {
        color: action.color,
        move_number: action.move_number,
        coordinate: action.coordinate.clone(),
        winrate_loss: positive(win),
        score_loss: score.map(positive),
        first_choice: matched == Some(0),
        ai_rank: matched.map(|i| i as u32).unwrap_or(999),
        category: category(equivalent),
        move_rank: display::classify(settings, -positive(win), score.map(|s| -positive(s))),
        score_equivalent_loss: equivalent,
        complexity,
        adjusted_weight: weight(complexity, equivalent),
        human: None,
    })
}
pub fn summarize_samples(samples: &[StrengthSample], model: &StrengthModel) -> StrengthSideReport {
    let summary = summary(samples, model);
    let n = samples.len();
    let minimum = if model.human_sl {
        6
    } else if n < 16 {
        3
    } else if n < 40 {
        4
    } else {
        5
    };
    let segment_size = if n <= minimum * 2 {
        n.max(1)
    } else {
        minimum.max((n as f64 / ((n as f64).sqrt().round() as usize).clamp(2, 9) as f64).ceil() as usize)
    };
    let mut sorted = samples.to_vec();
    sorted.sort_by_key(|s| s.move_number);
    let mut segments = Vec::new();
    let mut start = 0;
    while start < n {
        let mut end = n.min(start + segment_size);
        if n > end && n - end < minimum {
            end = n;
        }
        let slice = &sorted[start..end];
        segments.push(StrengthSegment {
            first_move: slice[0].move_number,
            last_move: slice[slice.len() - 1].move_number,
            summary: summary_fn(slice, model),
        });
        start = end;
    }
    StrengthSideReport {
        summary,
        samples: sorted,
        segments,
        minimum_segment_samples: minimum as u32,
    }
}
fn summary_fn(s: &[StrengthSample], m: &StrengthModel) -> StrengthSummary {
    summary(s, m)
}
fn quantile(values: &mut [f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let position = p * (values.len() - 1) as f64;
    let lower = position.floor() as usize;
    let upper = position.ceil() as usize;
    if lower == upper {
        values[lower]
    } else {
        let w = position - lower as f64;
        values[lower] * (1.0 - w) + values[upper] * w
    }
}
fn weighted(samples: &[&StrengthSample], fallback: f64) -> f64 {
    if samples.is_empty() {
        return fallback;
    }
    let mut sum = 0.0;
    let mut weights = 0.0;
    for s in samples {
        sum += s.score_equivalent_loss * s.adjusted_weight;
        weights += s.adjusted_weight;
    }
    if weights == 0.0 {
        fallback
    } else {
        sum / weights
    }
}
fn summary(samples: &[StrengthSample], model: &StrengthModel) -> StrengthSummary {
    let n = samples.len();
    if n == 0 {
        return StrengthSummary {
            sample_count: 0,
            score_sample_count: 0,
            rank_value: if model.human_sl { None } else { Some(0.0) },
            prediction_sigma: None,
            quality_score: Some(0.0),
            average_winrate_loss: 0.0,
            average_score_loss: 0.0,
            median_score_loss: 0.0,
            weighted_score_loss: 0.0,
            average_score_equivalent_loss: 0.0,
            match_rate: 0.0,
            first_choice_rate: 0.0,
            good_move_rate: 0.0,
            move_rank_good_move_rate: 0.0,
            move_rank_counts: vec![0; 6],
            bad_move_rate: 0.0,
            mistake_rate: 0.0,
            blunder_rate: 0.0,
            average_difficulty: Some(0.0),
            strength_band: "-".into(),
            confidence: StrengthConfidence::Low,
            human_sl_coverage: None,
        };
    }
    let mut win_sum = 0.0;
    let mut score_sum = 0.0;
    let mut equivalent_sum = 0.0;
    let mut weighted_sum = 0.0;
    let mut weight_sum = 0.0;
    let mut complexity_sum = 0.0;
    let mut scores = Vec::new();
    let mut losses = Vec::new();
    let mut phases: [Vec<&StrengthSample>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut counts = vec![0u32; 6];
    let mut rank_sum = 0.0;
    let (
        mut first,
        mut top3,
        mut top5,
        mut excellent,
        mut good,
        mut rank_good,
        mut bad,
        mut inaccuracies,
        mut mistakes,
        mut blunders,
    ) = (0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    for s in samples {
        win_sum += positive(s.winrate_loss);
        let loss = positive(s.score_equivalent_loss);
        equivalent_sum += positive(loss);
        losses.push(loss);
        weighted_sum += loss * s.adjusted_weight;
        weight_sum += s.adjusted_weight;
        if let Some(v) = s.score_loss {
            score_sum += positive(v);
            scores.push(positive(v));
        }
        if s.first_choice {
            first += 1;
        }
        if s.ai_rank < 3 {
            top3 += 1;
        }
        if s.ai_rank < 5 {
            top5 += 1;
        }
        if s.category == StrengthMoveCategory::Excellent {
            excellent += 1;
        }
        if s.category.good() {
            good += 1;
        } else {
            bad += 1;
        }
        counts[s.move_rank as usize] += 1;
        if matches!(s.move_rank, MoveRank::Best | MoveRank::Good) {
            rank_good += 1;
        }
        if s.category == StrengthMoveCategory::Inaccuracy {
            inaccuracies += 1;
        }
        if s.category.mistake() {
            mistakes += 1;
        }
        if s.category == StrengthMoveCategory::Blunder {
            blunders += 1;
        }
        phases[if s.move_number <= 60 {
            0
        } else if s.move_number <= 160 {
            1
        } else {
            2
        }]
        .push(s);
        rank_sum += if s.ai_rank >= 999 {
            10.0
        } else {
            (s.ai_rank as f64 + 1.0).min(10.0)
        };
        complexity_sum += s.complexity;
    }
    let count = n as f64;
    let sc = scores.len();
    let average = equivalent_sum / count;
    let score_average = if sc == 0 { 0.0 } else { score_sum / sc as f64 };
    let median = quantile(&mut scores, 0.5);
    let p75 = quantile(&mut losses, 0.75);
    let p90 = quantile(&mut losses, 0.90);
    let p95 = quantile(&mut losses, 0.95);
    // Java percentile sorts this list before populationStddev, so retain that summation order.
    let mean = losses.iter().sum::<f64>() / count;
    let stddev = (losses
        .iter()
        .map(|v| {
            let d = v - mean;
            d * d
        })
        .sum::<f64>()
        / count)
        .sqrt();
    let weighted = if weight_sum == 0.0 {
        average
    } else {
        weighted_sum / weight_sum
    };
    let first = first as f64 / count;
    let top3 = top3 as f64 / count;
    let top5 = top5 as f64 / count;
    let excellent = excellent as f64 / count;
    let good = good as f64 / count;
    let mistake = mistakes as f64 / count;
    let blunder = blunders as f64 / count;
    let inaccuracy = inaccuracies as f64 / count;
    let match_rate = clamp(0.45 * first + 0.45 * good + 0.10 * (1.0 - mistake), 0.0, 1.0);
    let mut difficulty = complexity_sum * 100.0 / count;
    let median_model = if sc == 0 { average } else { median };
    let difficulty_fit = clamp((difficulty - 25.0) / 35.0, 0.0, 1.0);
    let fit = |loss: f64, cap: f64| 1.0 / (1.0 + clamp(positive(loss), 0.0, cap));
    let phase_good = |i: usize| {
        if phases[i].is_empty() {
            good
        } else {
            phases[i].iter().filter(|s| s.category.good()).count() as f64 / phases[i].len() as f64
        }
    };
    let features = [
        first,
        top3,
        top5,
        1.0 / (1.0 + clamp(rank_sum / count, 0.0, 10.0)),
        excellent,
        good,
        1.0 - inaccuracy,
        match_rate,
        1.0 - mistake,
        1.0 - blunder,
        fit(weighted, 50.0),
        fit(average, 50.0),
        fit(median_model, 50.0),
        fit(p75, 50.0),
        fit(p90, 80.0),
        fit(p95, 100.0),
        fit(losses.iter().copied().fold(0.0, f64::max), 120.0),
        fit(stddev, 50.0),
        difficulty_fit,
        fit(weighted_fn(&phases[0], weighted), 50.0),
        fit(weighted_fn(&phases[1], weighted), 50.0),
        fit(weighted_fn(&phases[2], weighted), 50.0),
        phase_good(0),
        phase_good(1),
        phase_good(2),
        first * difficulty_fit,
        good * difficulty_fit,
        match_rate * difficulty_fit,
        top5 * difficulty_fit,
    ];
    let human: Vec<_> = samples.iter().filter_map(|s| s.human.clone()).collect();
    let rank = if model.human_sl {
        if human.len() == n {
            humansl40::predict(&human).ok().and_then(finite)
        } else {
            None
        }
    } else {
        model.predict(&features).and_then(finite)
    };
    let sigma = if model.human_sl {
        None
    } else {
        finite(model.prediction_sigma)
    };
    let quality = rank.map(|r| clamp((r + 18.0) * 100.0 / 30.0, 0.0, 100.0));
    if model.human_sl {
        difficulty = humansl40::java_mean(
            &human
                .iter()
                .map(|h| h.human[2][1])
                .filter(|v| v.is_finite())
                .collect::<Vec<_>>(),
        ) * 100.0;
    }
    let coverage = if human.is_empty() {
        None
    } else {
        finite(humansl40::java_mean(
            &human.iter().map(|h| h.human[2][2]).collect::<Vec<_>>(),
        ))
    };
    let band = if model.human_sl {
        rank.map(|r| BANDS[level(r)]).unwrap_or("-").to_owned()
    } else {
        strength_band(
            n,
            quality.unwrap_or(f64::NAN),
            weighted,
            average,
            median_model,
            p90,
            first,
            good,
            mistake,
            match_rate,
            difficulty,
        )
        .into()
    };
    StrengthSummary {
        sample_count: n as u32,
        score_sample_count: sc as u32,
        rank_value: rank,
        prediction_sigma: sigma,
        quality_score: quality,
        average_winrate_loss: win_sum / count,
        average_score_loss: score_average,
        median_score_loss: median,
        weighted_score_loss: weighted,
        average_score_equivalent_loss: average,
        match_rate,
        first_choice_rate: first,
        good_move_rate: good,
        move_rank_good_move_rate: rank_good as f64 / count,
        move_rank_counts: counts,
        bad_move_rate: bad as f64 / count,
        mistake_rate: mistake,
        blunder_rate: blunder,
        average_difficulty: finite(difficulty),
        strength_band: band,
        confidence: if model.human_sl {
            StrengthConfidence::Low
        } else if n >= 40 && sc as f64 >= count * 0.7 {
            StrengthConfidence::High
        } else if n >= 16 {
            StrengthConfidence::Medium
        } else {
            StrengthConfidence::Low
        },
        human_sl_coverage: coverage,
    }
}
fn weighted_fn(s: &[&StrengthSample], fallback: f64) -> f64 {
    weighted(s, fallback)
}
const BANDS: [&str; 14] = [
    "Beginner",
    "11-15k",
    "6-10k",
    "3-5k",
    "1-2k",
    "1-2d",
    "3-4d",
    "5-6d",
    "7d",
    "8d",
    "9d",
    "10d职业",
    "11d一线职业",
    "12d AI",
];
fn level(rank: f64) -> usize {
    for (threshold, level) in [
        (11.5, 13),
        (10.5, 12),
        (9.5, 11),
        (8.5, 10),
        (7.5, 9),
        (6.5, 8),
        (4.5, 7),
        (2.5, 6),
        (0.0, 5),
        (-2.75, 4),
        (-6.0, 3),
        (-10.5, 2),
        (-15.5, 1),
    ] {
        if rank >= threshold {
            return level;
        }
    }
    0
}
fn at_least(value: f64, thresholds: &[(f64, usize)], fallback: usize) -> usize {
    thresholds
        .iter()
        .find(|(v, _)| value >= *v)
        .map(|(_, l)| *l)
        .unwrap_or(fallback)
}
fn at_most(value: f64, thresholds: &[(f64, usize)], fallback: usize) -> usize {
    thresholds
        .iter()
        .find(|(v, _)| value <= *v)
        .map(|(_, l)| *l)
        .unwrap_or(fallback)
}
#[allow(clippy::too_many_arguments)]
fn strength_band(
    n: usize,
    quality: f64,
    w: f64,
    average: f64,
    median: f64,
    p90: f64,
    first: f64,
    good: f64,
    mistake: f64,
    matched: f64,
    difficulty: f64,
) -> &'static str {
    let elite = if first >= 0.95 && good >= 0.96 && mistake <= 0.01 && w <= 1.0 {
        13
    } else if first >= 0.50 && good >= 0.86 && mistake <= 0.05 && w <= 3.20 {
        12
    } else if first >= 0.45 && good >= 0.86 && mistake <= 0.06 && w <= 3.0 {
        11
    } else if first >= 0.44 && good >= 0.78 && matched >= 0.62 && mistake <= 0.03 && w <= 2.0 {
        10
    } else {
        0
    };
    let mut cap = 13usize;
    cap = cap.min(at_least(
        first,
        &[
            (0.62, 13),
            (0.50, 12),
            (0.46, 11),
            (0.42, 10),
            (0.38, 9),
            (0.34, 8),
            (0.30, 7),
            (0.24, 7),
            (0.18, 6),
            (0.12, 5),
            (0.06, 4),
        ],
        6,
    ));
    cap = cap.min(at_least(
        good,
        &[
            (0.96, 13),
            (0.88, 12),
            (0.82, 11),
            (0.78, 10),
            (0.74, 9),
            (0.68, 8),
            (0.62, 7),
            (0.56, 6),
            (0.50, 5),
            (0.42, 4),
            (0.32, 3),
            (0.22, 2),
        ],
        1,
    ));
    cap = cap.min(at_least(
        matched,
        &[
            (0.80, 13),
            (0.70, 12),
            (0.62, 11),
            (0.58, 10),
            (0.54, 9),
            (0.49, 8),
            (0.43, 7),
            (0.37, 6),
            (0.30, 5),
            (0.22, 4),
            (0.14, 3),
        ],
        2,
    ));
    cap = cap.min(at_most(
        mistake,
        &[
            (0.005, 13),
            (0.050, 12),
            (0.060, 11),
            (0.070, 10),
            (0.100, 9),
            (0.140, 8),
            (0.180, 7),
            (0.240, 6),
            (0.310, 5),
            (0.400, 4),
            (0.520, 3),
            (0.660, 2),
        ],
        1,
    ));
    cap = cap.min(at_most(
        median,
        &[
            (0.05, 13),
            (0.25, 12),
            (0.50, 11),
            (0.80, 10),
            (1.10, 9),
            (1.50, 8),
            (2.00, 7),
            (2.60, 6),
            (3.40, 5),
            (4.60, 4),
            (6.50, 3),
            (9.00, 2),
        ],
        1,
    ));
    if (good < 0.50 && first < 0.22) || (good < 0.62 && first < 0.30) || (w > 7.50 && first < 0.26) {
        cap = cap.min(4);
    }
    if w >= 2.90 && first < 0.34 && good < 0.78 {
        cap = cap.min(6);
    }
    let mut result = level(-18.0 + clamp(quality, 0.0, 100.0) * 30.0 / 100.0)
        .max(elite)
        .min(cap);
    if p90 > 8.0 && average > 2.8 && matched < 0.55 {
        result = result.min(7);
    }
    if p90 > 5.5 && median > 1.0 && matched < 0.50 {
        result = result.min(6);
    }
    if w > 4.8 && average > 3.0 && matched < 0.45 {
        result = result.min(4);
    }
    if result >= 9 && difficulty < 25.0 && first < 0.35 && good < 0.74 {
        result = result.min(7);
    }
    if n < 6 && result == 0 && w <= 10.0 {
        result = 1;
    }
    BANDS[result]
}
